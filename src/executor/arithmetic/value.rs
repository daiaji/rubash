use super::{ArithLValue, ConditionalArithParser};
use crate::executor::arithmetic::{
    bash_arith, checked_arithmetic_pow, eval_mutable_arith_value_with_random,
    strip_arith_double_quotes,
};
use crate::executor::{
    array_value_at, assoc_entries, assoc_value_at, current_epoch_seconds,
    env_derived_dynamic_parameter_value, format_assoc_storage, format_indexed_array_storage,
    indexed_array_entries, is_marked_var, is_noassign_bash_array, is_shell_name,
    mark_env_name, next_random_from_state, next_srandom_from_state, resolve_indexed_array_subscript,
    parse_array_subscript, set_process_env, ARRAY_VARS, ASSOC_VARS, NAMEREF_VARS,
    READONLY_VARS, SECONDS_OFFSET, SHELL_START_EPOCH,
};

impl ConditionalArithParser<'_> {
    pub(super) fn lvalue_value(&mut self, lvalue: &ArithLValue) -> Option<i128> {
        match lvalue {
            ArithLValue::Scalar(name) => self.variable_value(name),
            ArithLValue::Indexed { name, index } => {
                let value = self.env_vars.get(name).and_then(|value| {
                    resolve_indexed_array_subscript(value, *index)
                        .and_then(|index| array_value_at(value, index))
                });
                let value = value.unwrap_or_default();
                self.evaluate_variable_text(&format!("{name}[{index}]"), &value)
            }
            ArithLValue::IndexedRaw { .. } => {
                // Should have been resolved by resolve_raw_subscript before
                // reaching here; treat as a value fetch failure.
                None
            }
            ArithLValue::Assoc { name, key } => {
                let value = self
                    .env_vars
                    .get(name)
                    .and_then(|value| assoc_value_at(value, key))
                    .unwrap_or_default();
                self.evaluate_variable_text(&format!("{name}[{key}]"), &value)
            }
        }
    }

    pub(super) fn variable_value(&mut self, name: &str) -> Option<i128> {
        if self.resolving.iter().any(|resolving| resolving == name) {
            return None;
        }
        if name == "RANDOM" {
            return self
                .random_state
                .map(|state| i128::from(next_random_from_state(state)));
        }
        if name == "SRANDOM" {
            return self
                .random_state
                .map(|state| i128::from(next_srandom_from_state(state)));
        }
        if name == "LINENO" {
            return self
                .env_vars
                .get("__RUBASH_CURRENT_LINE")
                .and_then(|line| line.parse::<i128>().ok())
                .or(Some(1));
        }
        // Dynamic parameters ($SECONDS, $EPOCHSECONDS, ...) never have a
        // stored env_vars entry, so the fallback below would read them as 0.
        // Resolve them through the same path parameter expansion uses.
        if let Some(value) = env_derived_dynamic_parameter_value(self.env_vars, name) {
            if let Ok(number) = value.parse::<i128>() {
                return Some(bash_arith(number));
            }
        }

        // GNU expr.c treats a bare indexed-array operand as element zero.
        // The typed store serializes the whole array, which is not itself an
        // arithmetic expression, so resolve the scalar view before parsing.
        if is_marked_var(self.env_vars, ARRAY_VARS, name) {
            if let Some(value) = self
                .env_vars
                .get(name)
                .and_then(|value| array_value_at(value, 0))
            {
                return self.evaluate_variable_text(name, &value);
            }
        }

        let value = self
            .env_vars
            .get(name)
            .cloned()
            .or_else(|| std::env::var(name).ok())
            .unwrap_or_default();
        self.evaluate_variable_text(name, &value)
    }

    pub(super) fn evaluate_variable_text(
        &mut self,
        resolving_name: &str,
        value: &str,
    ) -> Option<i128> {
        if self
            .resolving
            .iter()
            .any(|resolving| resolving == resolving_name)
        {
            return None;
        }

        let value = value.trim();
        if value.is_empty() {
            return Some(0);
        }
        if let Ok(number) = value.parse::<i128>() {
            return Some(bash_arith(number));
        }

        let mut resolving = self.resolving.clone();
        resolving.push(resolving_name.to_string());
        let mut parser = ConditionalArithParser {
            input: value.as_bytes(),
            pos: 0,
            env_vars: self.env_vars,
            resolving,
            random_state: self.random_state,
            error_category: None,
            no_expand: false,
        };
        let value = parser.parse_comma()?;
        parser.skip_ws();
        let category = parser.error_category;
        if category.is_some() {
            self.error_category = category;
        }
        (parser.pos == parser.input.len()).then_some(value)
    }

    pub(super) fn update_lvalue(
        &mut self,
        lvalue: &ArithLValue,
        delta: i128,
        prefix: bool,
    ) -> Option<i128> {
        if !self.lvalue_is_writable(lvalue) {
            return None;
        }
        let current = self.lvalue_value(lvalue)?;
        let updated = bash_arith(current + delta);
        self.set_lvalue(lvalue, updated);
        Some(if prefix { updated } else { current })
    }

    pub(super) fn assign_lvalue(
        &mut self,
        lvalue: &ArithLValue,
        op: &str,
        rhs: i128,
    ) -> Option<i128> {
        // Resolve a deferred (raw) subscript now — after the RHS has been
        // evaluated, so side effects in the RHS are visible to the subscript
        // (GNU expr.c:1395-1401 + expr_bind_variable re-evaluation).
        let lvalue = self.resolve_raw_subscript(lvalue)?;
        if !self.lvalue_is_writable(&lvalue) {
            return None;
        }
        if op == "=" {
            self.set_lvalue(&lvalue, rhs);
            return Some(rhs);
        }
        let current = self.lvalue_value(&lvalue)?;
        let value = match op {
            "+=" => bash_arith(current + rhs),
            "-=" => bash_arith(current - rhs),
            "*=" => bash_arith(current * rhs),
            "**=" => checked_arithmetic_pow(current, rhs)?,
            "<<=" => bash_arith((current as i64).wrapping_shl(u32::try_from(rhs).ok()?) as i128),
            ">>=" => bash_arith((current as i64).wrapping_shr(u32::try_from(rhs).ok()?) as i128),
            "&=" => bash_arith(current & rhs),
            "^=" => bash_arith(current ^ rhs),
            "|=" => bash_arith(current | rhs),
            "/=" if rhs != 0 => bash_arith((current as i64).wrapping_div(rhs as i64) as i128),
            "%=" if rhs != 0 => {
                // GNU expr.c:923-926: INTMAX_MIN % -1 is 0.
                if current == i128::from(i64::MIN) && rhs == -1 {
                    0
                } else {
                    current % rhs
                }
            }
            "/=" | "%=" => return None,
            _ => return None,
        };
        self.set_lvalue(&lvalue, value);
        Some(value)
    }

    /// Evaluate a deferred raw subscript into a concrete `Indexed` lvalue.
    /// Non-raw lvalues pass through unchanged.
    fn resolve_raw_subscript(&mut self, lvalue: &ArithLValue) -> Option<ArithLValue> {
        match lvalue {
            ArithLValue::IndexedRaw { name, subscript } => {
                let stripped = strip_arith_double_quotes(subscript);
                if stripped.trim().is_empty() {
                    return Some(ArithLValue::Indexed {
                        name: name.clone(),
                        index: 0,
                    });
                }
                let (value, _cat) = eval_mutable_arith_value_with_random(
                    &stripped,
                    self.env_vars,
                    self.random_state,
                );
                if value.is_none() {
                    // GNU expr.c evalerror from the nested subscript evalexp
                    // (array_expand_index) reports the subscript text.
                    self.env_vars.insert(
                        "__RUBASH_ARITH_SUBSCRIPT_EXPR".to_string(),
                        stripped.clone(),
                    );
                }
                Some(ArithLValue::Indexed {
                    name: name.clone(),
                    index: value?,
                })
            }
            other => Some(other.clone()),
        }
    }

    fn lvalue_is_writable(&mut self, lvalue: &ArithLValue) -> bool {
        let name = match lvalue {
            ArithLValue::Scalar(name)
            | ArithLValue::Indexed { name, .. }
            | ArithLValue::IndexedRaw { name, .. }
            | ArithLValue::Assoc { name, .. } => name,
        };
        if is_marked_var(self.env_vars, READONLY_VARS, name) {
            self.env_vars
                .insert("__RUBASH_ARITH_READONLY_ERROR".to_string(), name.clone());
            return false;
        }
        true
    }

    pub(super) fn set_lvalue(&mut self, lvalue: &ArithLValue, value: i128) {
        match lvalue {
            ArithLValue::Scalar(name) => self.set_variable(name, value),
            ArithLValue::Indexed { name, index } => self.set_array_element(name, *index, value),
            ArithLValue::IndexedRaw { .. } => {
                // Should have been resolved by resolve_raw_subscript; no-op.
            }
            ArithLValue::Assoc { name, key } => self.set_assoc_element(name, key, value),
        }
    }

    pub(super) fn set_variable(&mut self, name: &str, value: i128) {
        if is_noassign_bash_array(name) {
            return;
        }
        let value = bash_arith(value).to_string();
        if name == "SECONDS" {
            // Assignment resets the reference point so the dynamic value
            // becomes the assigned number and grows from there, matching
            // the parameter-assignment path in temporary_assignments.rs.
            let assigned = value.parse::<i64>().unwrap_or(0);
            let start = self
                .env_vars
                .get(SHELL_START_EPOCH)
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_else(current_epoch_seconds);
            let elapsed = current_epoch_seconds() - start;
            self.env_vars
                .insert(SECONDS_OFFSET.to_string(), (assigned - elapsed).to_string());
            set_process_env(name, value);
            return;
        }
        if name == "RANDOM" {
            if let Some(state) = self.random_state {
                state.set(value.parse::<u32>().unwrap_or(0));
            }
        }
        if name == "SRANDOM" {
            return;
        }
        // GNU expr.c assigns through bind_variable: a nameref lvalue resolves
        // to its cell — an empty cell adopts the assigned text after
        // valid_nameref_value (invalid -> sh_invalidid via the marker below),
        // an already-invalid cell stays unchanged, and a valid cell forwards
        // the write to the referenced variable or element.
        if is_marked_var(self.env_vars, NAMEREF_VARS, name) {
            let cell = self.env_vars.get(name).cloned().unwrap_or_default();
            let cell_valid = is_shell_name(&cell)
                || parse_array_subscript(&cell).is_some();
            if !cell_valid {
                if cell.is_empty() {
                    if is_shell_name(&value)
                        || parse_array_subscript(&value).is_some()
                    {
                        let old_value = self.env_vars.get(name).cloned();
                        self.env_vars.insert(name.to_string(), value.clone());
                        super::super::record_arith_write(name, old_value);
                        set_process_env(name, value);
                    } else {
                        self.env_vars.insert(
                            "__RUBASH_ARITH_NAMEREF_ERROR".to_string(),
                            value,
                        );
                    }
                }
                return;
            }
            // Follow the chain to the last resolvable cell (NAMEREF_MAX=8).
            let mut target = cell;
            for _ in 0..8 {
                let base = target.split('[').next().unwrap_or(target.as_str());
                if !is_marked_var(self.env_vars, NAMEREF_VARS, base) {
                    break;
                }
                let next = self.env_vars.get(base).cloned().unwrap_or_default();
                if next.is_empty() || next == target {
                    break;
                }
                target = next;
            }
            if let Some((elem_base, subscript)) = target.split_once('[') {
                if let Some(subscript) = subscript.strip_suffix(']') {
                    let stripped = strip_arith_double_quotes(subscript);
                    let (index, _cat) = eval_mutable_arith_value_with_random(
                        &stripped,
                        self.env_vars,
                        self.random_state,
                    );
                    if let Some(index) = index {
                        self.set_array_element(elem_base, index, value.parse::<i128>().unwrap_or(0));
                        return;
                    }
                }
            }
            let base_target = target.split('[').next().unwrap_or(target.as_str());
            if is_marked_var(self.env_vars, READONLY_VARS, base_target) {
                self.env_vars.insert(
                    "__RUBASH_ARITH_READONLY_ERROR".to_string(),
                    base_target.to_string(),
                );
                return;
            }
            let numeric = value.parse::<i128>().unwrap_or(0);
            self.set_variable(&base_target.to_string(), numeric);
            return;
        }
        let old_value = self.env_vars.get(name).cloned();
        self.env_vars.insert(name.to_string(), value.clone());
        super::super::record_arith_write(name, old_value);
        set_process_env(name, value);
    }

    pub(super) fn set_array_element(&mut self, name: &str, index: i128, value: i128) {
        if is_noassign_bash_array(name) {
            return;
        }
        let mut entries = self
            .env_vars
            .get(name)
            .map(|value| indexed_array_entries(value))
            .unwrap_or_default();
        let index = if index < 0 {
            let storage = format_indexed_array_storage(entries.clone());
            let Some(index) = resolve_indexed_array_subscript(&storage, index) else {
                return;
            };
            index
        } else {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            index
        };
        entries.insert(index, value.to_string());
        let value = format_indexed_array_storage(entries);
        let old_value = self.env_vars.get(name).cloned();
        self.env_vars.insert(name.to_string(), value);
        super::super::record_arith_write(name, old_value);
        mark_env_name(self.env_vars, ARRAY_VARS, name);
    }

    pub(super) fn set_assoc_element(&mut self, name: &str, key: &str, value: i128) {
        let mut entries = self
            .env_vars
            .get(name)
            .map(|value| assoc_entries(value))
            .unwrap_or_default();
        let value = value.to_string();
        if let Some((_, existing)) = entries.iter_mut().find(|(entry_key, _)| entry_key == key) {
            *existing = value;
        } else {
            entries.push((key.to_string(), value));
        }
        let old_value = self.env_vars.get(name).cloned();
        self.env_vars
            .insert(name.to_string(), format_assoc_storage(entries));
        super::super::record_arith_write(name, old_value);
        mark_env_name(self.env_vars, ASSOC_VARS, name);
    }
}
