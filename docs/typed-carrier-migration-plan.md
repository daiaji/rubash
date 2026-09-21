# Typed Carrier Migration Plan

**Status**: Phase 1 - Inventory and Batch Planning
**Reference**: docs/typed-expansion-migration-checkpoint.md (archived)
**Rule**: Governance 3.1/3.2 - No new markers; existing markers only gain collision fixes and missing consumers.

## Migration Strategy

**Goal**: Replace in-band sentinel bytes with structured carriers (ExpandedWord/ExpandedFragment types) to eliminate M-class accident root causes.

**Approach**: Incremental batches, not big-bang rewrite. Each batch:
1. Select one marker family (or single high-risk marker)
2. Introduce typed carrier field in relevant struct
3. Migrate write point to set typed field instead of writing sentinel
4. Migrate consume points to read typed field instead of stripping sentinel
5. Validate: cargo test + 83 suites true-baseline + golden assertions
6. Commit milestone

**Selection Criteria**:
- **Low risk first**: Few consumption points, function-local boundaries
- **High value**: Frequently-traveled paths (CTLESC family, DATA_* family)
- **Clear semantic**: Single-purpose markers (PATSUB family is function-local)

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

## Batch Plan

### Batch 1: PREEXPANDED_STDIN_BODY (Pilot)

**Marker**: `\u{5}` (ENQ, PREEXPANDED_STDIN_BODY)
**Risk**: Medium - execution-time, few consumption points (7), clear semantic
**Boundary**: Reparse
**Write Point**: `command_execute.rs::preexpand_command_stdin()` (lines 659, 674, 684)
**Consume Points**:
- `execution_misc.rs::preexpanded_stdin_body()` (line 291)
- `execution_misc.rs::decode_stdin_body_enq()` (line 301)
- 5 additional executor paths (heredoc owner chains)

**Typed Carrier Design**:
- Add `bool preexpanded` field to `HereDocBody` struct or create a `StdinBody { text: String, preexpanded: bool }` enum
- Replace `format!("{PREEXPANDED_STDIN_BODY}{}", ...)` with `StdinBody { text: ..., preexpanded: true }`
- Consume points check `body.preexpanded` instead of `body.starts_with(PREEXPANDED_STDIN_BODY)`

**Validation**:
- Golden assertion: `\u{5}` never appears in stdout/declare -p/xtrace
- Focused test: heredoc with literal 0x05 byte at start (raw-byte marker pair ensures collision safety)
- 83-suite true-baseline (no regression in heredoc/redir/read/mapfile slices)

### Batch 2: PATSUB Family (Low-Risk Function-Local)

**Markers**: PATSUB_QUOTED_VALUE_START/END/AMP/BACKSLASH
**Risk**: Low - function-local, single consumer
**Boundary**: None (substitution-internal)
**Write Point**: `expand_braced_replacement.rs` marking passes
**Consume Point**: `finish_patsub_replacement.rs` decode pass

**Typed Carrier Design**:
- Add `QuotedRegion { start: usize, end: usize }` to replacement metadata
- Replace marker insertion with region tracking
- Finish pass uses region ranges instead of marker stripping

### Batch 3: Function-Local Guards (Low Risk)

**Markers**: PARAM_WORD_BACKSLASH_GUARD, ESCAPED_IFS_GUARD, PROMPT_ESCAPE_GUARD, CASE_PATTERN_BACKSLASH_GUARD
**Risk**: Low - function-local, producer = consumer
**Boundary**: None (function-local)

**Typed Carrier Design**:
- Each guard becomes a typed boolean flag on the processing context struct
- Replace marker insertion with flag set
- Replace marker check with flag read

### Batch 4: ASSIGN_DATA_* Family (Storage Boundary)

**Markers**: ASSIGN_DATA_SQUOTE/DQUOTE/BACKTICK/ESCAPED_*/HOISTED_*/COMPOUND_EXPANSION_WS_TAG/ASSIGN_SQ_*
**Risk**: Medium - storage boundary only, single decode point
**Boundary**: Storage
**Write Point**: `assignment_expansion.rs`
**Consume Point**: `locale.rs::decode_to_visible_text`

**Typed Carrier Design**:
- Extend `ExpandedFragment` with `quote_provenance: QuoteProvenance` enum
- Replace ASSIGN_DATA_* markers with `QuoteProvenance::DataSQuote`, etc.
- Storage decoder reads provenance instead of marker stripping

### Batch 5: CTLESC Family (High Risk - Most Traveled)

**Marker**: CTLESC
**Risk**: High - many consumption points, lexer→parser→executor pipeline
**Boundary**: Output
**Write Points**: lexer/quotes.rs (glob chars), embedded_parameters.rs
**Consume Points**: locale.rs decode, pipeline_exec.rs (x3), conditional_command.rs, case_command.rs

**Typed Carrier Design**:
- Extend `Token` or `WordMetadata` with `protected_chars: Vec<usize>` list
- Replace CTLESC+char insertion with protected char index list
- Consume points strip protected chars by index instead of marker scanning

### Batch 6: Named String Markers

**Markers**: QUOTED_HEREDOC_MARKER, COMSUB_PAYLOAD_PREFIX, COMPOUND_ASSIGNMENT_MARKER
**Risk**: Medium - multi-char prefix protocol
**Boundary**: Output/Reparse

**Typed Carrier Design**:
- QUOTED_HEREDOC_MARKER → bool field on HereDocBody token
- COMSUB_PAYLOAD_PREFIX → dedicated `ComsubPayload { bytes: Vec<u8>, status: i32 }` struct
- COMPOUND_ASSIGNMENT_MARKER → bool field on CompoundAssignment operand

## Current Task

**Phase 1**: Inventory complete - registered 35 markers across 6 families
**Next Step**: Create typed carrier migration plan document (this file) and commit milestone

**Pending**:
- [ ] Review and approve batch plan
- [ ] Begin Batch 1: PREEXPANDED_STDIN_BODY pilot migration
- [ ] Validate Batch 1 with golden assertions and 83-suite baseline
