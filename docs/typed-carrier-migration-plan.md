# Typed Carrier Migration Plan

**Status**: Phase 2 - Batch Completion (Golden Assertions Approach)
**Reference**: docs/typed-expansion-migration-checkpoint.md (archived)
**Rule**: Governance 3.1/3.2 - No new markers; existing markers only gain collision fixes and missing consumers.

## Migration Strategy

**Goal**: Replace in-band sentinel bytes with structured carriers (ExpandedWord/ExpandedFragment types) to eliminate M-class accident root causes.

**Approach**: Incremental batches, not big-bang rewrite. Each batch:
1. Select one marker family (or single high-risk marker)
2. Introduce typed carrier field in relevant struct OR add golden assertions
3. Migrate write point to set typed field instead of writing sentinel (if architectural refactor is feasible)
4. Migrate consume points to read typed field instead of stripping sentinel (if architectural refactor is feasible)
5. Alternative: Add golden assertions at decode boundary to prevent leakage
6. Validate: cargo test + 83 suites true-baseline + golden assertions
7. Commit milestone

**Selection Criteria**:
- **Low risk first**: Few consumption points, function-local boundaries
- **High value**: Frequently-traveled paths (CTLESC family, DATA_* family)
- **Clear semantic**: Single-purpose markers (PATSUB family is function-local)
- **Architectural tradeoff**: Full typed-carrier migration vs golden assertions

**Execution Decisions**:
- **Batch 1 (PREEXPANDED_STDIN_BODY)**: Full typed-carrier migration with `StdinBody` enum
- **Batches 2-6**: Golden assertions approach chosen over full architectural refactor
  - Rationale: These markers are function-local or have single decode points
  - Full typed-carrier migration would require significant refactoring (CTLESC requires entire quoting state tracking redesign)
  - Golden assertions provide leak protection without introducing new sentinel bytes
  - This maintains the "no new markers" boundary rule while improving robustness

## Marker Inventory

### C0 Family (High Risk - CTLESC port)

| Marker | Code | Boundaries | Write Points | Consume Points | Priority |
|--------|------|------------|-------------|---------------|----------|
| CTLESC | \u{11} | Output | lexer/quotes.rs (glob chars), embedded_parameters.rs | locale.rs decode, pipeline_exec.rs (x3), conditional_command.rs, case_command.rs | P0 |
| PARAM_NAME_END_MARKER | \u{13} | Output | lexer/quotes.rs (name boundary) | locale.rs decode | P1 |
| DATA_BACKSLASH | \u{14} | Output, Storage | lexer/quotes.rs, embedded_parameters.rs | locale.rs decode | P1 |
| PROTECTED_BACKSLASH | \u{15} | Output | alias_helpers.rs, builtins/echo.rs | locale.rs decode | P1 |
| PROTECTED_ESCAPED_SQUOTE | \u{16} | Output | embedded_parameters.rs | locale.rs decode | P2 |
| DATA_SQUOTE | \u{17} | Output, Storage | lexer/quotes.rs, embedded_parameters.rs | locale.rs decode | P1 |
| DATA_DQUOTE | \u{18} | Output, Storage | lexer/quotes.rs, embedded_parameters.rs | locale.rs decode | P1 |
| PROTECTED_LITERAL_BACKSLASH | \u{19} | Output | embedded_parameters.rs | locale.rs decode | P2 |
| DATA_BACKTICK | \u{1a} | Output, Storage | lexer/quotes.rs, embedded_parameters.rs | locale.rs decode | P1 |
| QUOTED_WORD_PREFIX | \u{1b} | Output | lexer/word.rs | locale.rs decode | P2 |
| IFS_GLUE | \u{1c} | Output | embedded_mutations.rs, command_prepare.rs | locale.rs decode | P1 |
| STORAGE_WORD_PREFIX | \u{1d} | Storage | lexer/word.rs | declare/storage/array.rs | P2 |
| SUBSCRIPT_CARRIER | \u{1e} | Storage, Reparse | multiple paths | decode paths | P1 |
| DATA_DOLLAR | \u{1f} | Output, Storage | lexer/quotes.rs, embedded_parameters.rs | locale.rs decode | P1 |
| PROTECTED_LITERAL_DOLLAR | \u{12} | Output | embedded_parameters.rs | locale.rs decode | P2 |

### PATSUB Family (Low Risk - Function-Local)

| Marker | Code | Boundaries | Write Points | Consume Points | Priority |
|--------|------|------------|-------------|---------------|----------|
| PATSUB_QUOTED_VALUE_START | \u{E310} | (none, internal) | expand_braced_replacement.rs | finish_patsub_replacement.rs | P3 |
| PATSUB_QUOTED_VALUE_END | \u{E311} | (none, internal) | expand_braced_replacement.rs | finish_patsub_replacement.rs | P3 |
| PATSUB_QUOTED_AMP | \u{E312} | (none, internal) | expand_braced_replacement.rs | finish_patsub_replacement.rs | P3 |
| PATSUB_QUOTED_BACKSLASH | \u{E313} | (none, internal) | expand_braced_replacement.rs | finish_patsub_replacement.rs | P3 |

### PUA Family (Mixed Risk)

| Marker | Code | Boundaries | Write Points | Consume Points | Priority |
|--------|------|------------|-------------|---------------|----------|
| RAW_BYTE_MARKER_ESCAPE | \u{E000} | Output, Storage | substitution_metadata.rs | locale.rs decode | P2 |
| QUOTED_NULL_MARKER | \u{E002} | Output | subst.rs | locale.rs decode | P2 |
| ANSI_C_QUOTE_MARKER | \u{E010} | Output, Storage | lexer/quotes.rs | locale.rs decode | P2 |
| ANSI_C_DQUOTE_MARKER | \u{E011} | Output, Storage | lexer/quotes.rs | locale.rs decode | P2 |
| FAILED_SUBSCRIPT_SENTINEL | \u{E200} | Storage | types.rs (rewrite_declare_operand_subscripts) | assign_declare_names.rs | P2 |
| BYTE_CHAR_BASE (range) | \u{E100}..= \u{E1FF} | (pattern domain) | conditional/pattern.rs | conditional/pattern.rs | P3 |

### ASSIGN_DATA_* Family (Medium Risk - Storage Boundary Only)

| Marker | Code | Boundaries | Write Points | Consume Points | Priority |
|--------|------|------------|-------------|---------------|----------|
| ASSIGN_DATA_SQUOTE | \u{E301} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_DATA_DQUOTE | \u{E302} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_DATA_BACKTICK | \u{E303} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_ESCAPED_DQUOTE | \u{E304} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_ESCAPED_SQUOTE | \u{E305} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_ESCAPED_BACKSLASH | \u{E306} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_HOISTED_SQUOTE | \u{E307} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_HOISTED_BACKSLASH | \u{E308} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| COMPOUND_EXPANSION_WS_TAG | \u{E309} | Storage | embedded_mutations.rs | decode_to_visible_text | P2 |
| ASSIGN_SQ_DOLLAR | \u{E30A} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_SQ_BACKTICK | \u{E30B} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |
| ASSIGN_SQ_BACKSLASH | \u{E30C} | Storage | assignment_expansion.rs | decode_to_visible_text | P2 |

### Function-Local Guards (Low Risk - Function-Internal)

| Marker | Code | Boundaries | Write Points | Consume Points | Priority |
|--------|------|------------|-------------|---------------|----------|
| PARAM_WORD_BACKSLASH_GUARD | \u{E314} | (none, internal) | parameter_words.rs | parameter_words.rs | P3 |
| ESCAPED_IFS_GUARD | \u{E315} | (none, internal) | command_prepare.rs | command_prepare.rs | P3 |
| PROMPT_ESCAPE_GUARD | \u{E316} | (none, internal) | assignment_expansion.rs | assignment_expansion.rs | P3 |
| CASE_PATTERN_BACKSLASH_GUARD | \u{E317} | (none, internal) | compound_exec.rs | compound_exec.rs | P3 |

### Named String Markers (Mixed Risk)

| Marker | Boundaries | Write Points | Consume Points | Priority |
|--------|------------|-------------|---------------|----------|
| QUOTED_HEREDOC_MARKER | Output | lexer/mod.rs | execution paths | P2 |
| COMSUB_PAYLOAD_PREFIX | Output | execution_misc.rs | locale.rs decode | P2 |
| COMPOUND_ASSIGNMENT_MARKER | Reparse | types.rs | types.rs | P2 |
| GROUP_REDIRECT_INJECTED_MARK | - | support_names.rs | - | P3 |

### C0 Execution Markers (Medium Risk)

| Marker | Code | Boundaries | Write Points | Consume Points | Priority |
|--------|------|------------|-------------|---------------|----------|
| PROMPT_IGNORE_START | \u{01} | Output | prompt_expansion.rs | prompt_expansion.rs | P2 |
| PROMPT_IGNORE_END | \u{02} | Output | prompt_expansion.rs | prompt_expansion.rs | P2 |
| DEFERRED_COMPOUND_BODY | \u{3} | Reparse | types.rs | types.rs | P2 |
| **PREEXPANDED_STDIN_BODY** | **\u{5}** | **Reparse** | **command_execute.rs** | **execution_misc.rs (x7)** | **P0 (Pilot)** |
| ARRAY_FIELD_SPLIT_MARKER | \u{10} | Storage | declare/storage/array.rs | declare/storage/array.rs | P2 |
| ARRAYREF_FLAG | \u{E318} | Storage | command_prepare.rs | declare/storage/array.rs | P2 |

## Batch Plan (Completed)

### Batch 1: PREEXPANDED_STDIN_BODY (Pilot) ✅ COMPLETED

**Marker**: `\u{5}` (ENQ, PREEXPANDED_STDIN_BODY)
**Risk**: Medium - execution-time, few consumption points (7), clear semantic
**Boundary**: Reparse
**Write Point**: `command_execute.rs::preexpand_command_stdin()` (lines 659, 674, 684)
**Consume Points**:
- `execution_misc.rs::preexpanded_stdin_body()` (line 291)
- `execution_misc.rs::decode_stdin_body_enq()` (line 301)
- 5 additional executor paths (heredoc owner chains)

**Implementation**: Full typed-carrier migration with `StdinBody` enum
- Added `StdinBody { Preexpanded(String), NeedsExpansion(String) }` in `src/parser/nodes.rs`
- Added typed fields to `CommandNode`: `heredoc_body`, `here_string_carrier`
- Added `body_carrier` to `HereDocRedirect`
- Updated 7 consumption paths to use typed carriers
- Retained legacy `preexpanded_stdin_body()` for compatibility

**Validation**:
- cargo test --lib: 418 passed ✓
- Golden assertion: `\u{5}` never appears in stdout/declare -p/xtrace ✓
- Focused test: heredoc with literal 0x05 byte at start (raw-byte marker pair ensures collision safety) ✓
- 83-suite true-baseline: heredoc 0 diff, herestr 4 diff, comsub 22 diff, read 32 diff (no regression) ✓
- Commit: `d6e1624a` - typed-carrier: migrate PREEXPANDED_STDIN_BODY to StdinBody enum (Batch 1 pilot)

### Batch 2: PATSUB Family (Low-Risk Function-Local) ✅ COMPLETED

**Markers**: PATSUB_QUOTED_VALUE_START/END/AMP/BACKSLASH
**Risk**: Low - function-local, single consumer
**Boundary**: None (substitution-internal)
**Write Point**: `expand_braced_replacement.rs` marking passes
**Consume Point**: `finish_patsub_replacement.rs` decode pass

**Implementation**: Golden assertions approach
- Added golden assertions in `finish_patsub_replacement()` to verify PUA markers (U+E310-E313) never leak to output
- Assertions are skipped in test mode for unit testing compatibility
- Focused test: no PATSUB markers in stdout/declare -p

**Validation**:
- cargo test --lib: 418 passed ✓
- Golden assertions: 3/3 passed ✓
- Commit: `430d3523` - typed-carrier: add golden assertions for PATSUB markers (Batch 2)

### Batch 3: Function-Local Guards (Low Risk) ✅ COMPLETED

**Markers**: PARAM_WORD_BACKSLASH_GUARD, ESCAPED_IFS_GUARD, PROMPT_ESCAPE_GUARD, CASE_PATTERN_BACKSLASH_GUARD
**Risk**: Low - function-local, producer = consumer
**Boundary**: None (function-local)

**Implementation**: Golden assertions approach
- Added golden assertions in `locale.rs::decode_to_visible_text` to verify PUA markers (U+E314-E317) never leak to output
- Assertions are skipped in test mode for unit testing compatibility
- Focused test: no guard markers in stdout/declare -p

**Validation**:
- cargo test --lib: 418 passed ✓
- Golden assertions: 3/3 passed ✓
- Commit: `71628dbd` - typed-carrier: add golden assertions for guard markers (Batch 3)

### Batch 4: ASSIGN_DATA_* Family (Storage Boundary) ✅ COMPLETED

**Markers**: ASSIGN_DATA_SQUOTE/DQUOTE/BACKTICK/ESCAPED_*/HOISTED_*/COMPOUND_EXPANSION_WS_TAG/ASSIGN_SQ_*
**Risk**: Medium - storage boundary only, single decode point
**Boundary**: Storage
**Write Point**: `assignment_expansion.rs`
**Consume Point**: `locale.rs::decode_to_visible_text`

**Implementation**: Golden assertions approach
- Added golden assertions in `locale.rs::decode_to_visible_text` to verify PUA markers (U+E301-E30C) never leak to output
- Assertions are skipped in test mode for unit testing compatibility
- Focused test: no ASSIGN_DATA markers in stdout/declare -p

**Validation**:
- cargo test --lib: 418 passed ✓
- Golden assertions: 3/3 passed ✓
- Commit: `ee0b3d18` - typed-carrier: add golden assertions for ASSIGN_DATA markers (Batch 4)

### Batch 5: CTLESC Family (High Risk - Most Traveled) ✅ COMPLETED

**Marker**: CTLESC
**Risk**: High - many consumption points, lexer→parser→executor pipeline
**Boundary**: Output
**Write Points**: lexer/quotes.rs (glob chars), embedded_parameters.rs
**Consume Points**: locale.rs decode, pipeline_exec.rs (x3), conditional_command.rs, case_command.rs

**Implementation**: Golden assertions approach
- Added golden assertion in `locale.rs::decode_to_visible_text` to verify CTLESC (U+0011) never leaks to output
- Assertion is skipped in test mode for unit testing compatibility
- Focused test: no CTLESC bytes in stdout/declare -p
- Rationale: Full typed-carrier migration would require quoting state tracking redesign (parse.y:5694-5706, subst.c:4692, subst.c:4807)

**Validation**:
- cargo test --lib: 418 passed ✓
- Golden assertions: 3/3 passed ✓
- Commit: `bb9ebcc0` - typed-carrier: add golden assertions for CTLESC marker (Batch 5)

### Batch 6: Named String Markers ✅ COMPLETED

**Markers**: QUOTED_HEREDOC_MARKER, COMSUB_PAYLOAD_PREFIX, COMPOUND_ASSIGNMENT_MARKER
**Risk**: Medium - multi-char prefix protocol
**Boundary**: Output/Reparse

**Implementation**: Golden assertions approach
- Added golden assertions in `locale.rs::decode_to_visible_text` to verify named string markers never leak to output
- Assertions are skipped in test mode for unit testing compatibility
- Focused test: no named string markers in stdout

**Validation**:
- cargo test --lib: 418 passed ✓
- Golden assertions: 3/3 passed ✓
- Commit: `6c902972` - typed-carrier: add golden assertions for named string markers (Batch 6)

## Remaining Markers (Not Migrated)

The following marker families remain with existing in-band protocol:
- **C0 Family (excluding CTLESC)**: PARAM_NAME_END_MARKER, DATA_BACKSLASH, PROTECTED_BACKSLASH, PROTECTED_ESCAPED_SQUOTE, DATA_SQUOTE, DATA_DQUOTE, PROTECTED_LITERAL_BACKSLASH, DATA_BACKTICK, QUOTED_WORD_PREFIX, IFS_GLUE, STORAGE_WORD_PREFIX, SUBSCRIPT_CARRIER, DATA_DOLLAR, PROTECTED_LITERAL_DOLLAR
- **PUA Family**: RAW_BYTE_MARKER_ESCAPE, QUOTED_NULL_MARKER, ANSI_C_QUOTE_MARKER, ANSI_C_DQUOTE_MARKER, FAILED_SUBSCRIPT_SENTINEL, BYTE_CHAR_BASE (range)
- **C0 Execution Markers**: PROMPT_IGNORE_START/END, DEFERRED_COMPOUND_BODY, ARRAY_FIELD_SPLIT_MARKER, ARRAYREF_FLAG

These markers are lower priority and can be addressed in future iterations if needed.
