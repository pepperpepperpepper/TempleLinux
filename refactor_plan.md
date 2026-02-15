# Refactor Plan: Split `src/bin/temple_hc/01_vm.rs` (7,511 LOC)

## Goal
Break the current single-file VM implementation into a set of focused files so we:
- Avoid a growing monolith (merge conflicts + hard navigation).
- Make it easier to reason about “VM core” vs “host/builtins” vs “UI”.
- Keep behavior identical while we refactor (vendored TempleOS apps must keep running).

Non-goals (for this refactor):
- No semantic changes to HolyC execution.
- No changes to `third_party/TempleOS/` (treat as upstream).
- No big API redesign (yet).

## Status (as of 2026-02-12)
- ✅ **Phase 1 include-split completed** (no behavior changes):
  - `src/bin/temple_hc/01_vm.rs` was reduced to a tiny wrapper during Phase 1, and has since been deleted (the VM now builds as a real `crate::vm` module).
  - VM code moved into `src/bin/temple_hc/vm/*.rs` (12 files).
  - Note: Rust does **not** allow `include!()` directly inside an `impl` item list, so each fragment that holds methods now contains its own `impl Vm { ... }` block.
- ✅ **Builtins split completed (coarse)**:
  - `src/bin/temple_hc/vm/11_call.rs` now contains only `is_builtin` + a small `call` that delegates builtins to `Vm::call_builtin(...)`.
  - Builtins live in `src/bin/temple_hc/vm/builtins/*.rs` with a small dispatcher in `src/bin/temple_hc/vm/builtins/mod.rs`.
- ✅ **Phase 2 real-module conversion completed (VM)**:
  - `src/bin/temple_hc/vm/mod.rs` now uses `mod builtins;` (first real module boundary).
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::values` as a real module (`#[path = "00_values.rs"] mod values;`) and re-exports `Value` at the VM root.
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::env` as a real module (`#[path = "01_env.rs"] mod env;`) and imports its types into the VM root.
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::ui_types` as a real module (`#[path = "02_ui_types.rs"] mod ui_types;`) and imports its menu/UI types into the VM root.
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::vm_struct` as a real module (`#[path = "03_vm_struct.rs"] mod vm_struct;`) and re-exports `Vm` at the VM root.
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::init` as a real module (`#[path = "04_init.rs"] mod init;`) and keeps `Vm::new(...)` behind a module boundary.
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::heap_doldoc_rng` as a real module (`#[path = "05_heap_doldoc_rng.rs"] mod heap_doldoc_rng;`).
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::linux_bridge` as a real module (`#[path = "06_linux_bridge.rs"] mod linux_bridge;`).
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::exec` as a real module (`#[path = "07_exec.rs"] mod exec;`) and keeps `Vm::run(...)` behind a module boundary.
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::eval` as a real module (`#[path = "08_eval.rs"] mod eval;`).
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::ui` as a real module (`#[path = "09_ui.rs"] mod ui;`).
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::call` as a real module (`#[path = "11_call.rs"] mod call;`).
  - `src/bin/temple_hc/vm/mod.rs` now also declares `vm::text` as a real module (`#[path = "10_text.rs"] mod text;`).
  - `Vm::call_builtin(...)` is now `pub(super)` (needed since it now lives in a submodule).
  - `src/bin/temple_hc.rs` now declares a real `vm` module (`#[path = "temple_hc/vm/mod.rs"] mod vm;`) and no longer `include!()`s `temple_hc/01_vm.rs`.
  - VM entrypoints are now accessed via `crate::vm::Vm` / `crate::vm::Value` with a minimal crate-visible surface (`Vm`, `Value`, `Vm::{new,run,enable_capture,captured_output}`, `Value::{as_i64,as_f64}`).
  - Transitional glue: `src/bin/temple_hc/vm/mod.rs` defines a small `vm::prelude` (`mod prelude;`) so VM submodules can do `use super::prelude::*;` and avoid re-threading frontend/std imports everywhere.
  - Import cleanup: `src/bin/temple_hc/vm/builtins/mod.rs` no longer uses `use super::*;` (it now imports `super::prelude::*` + a small set of VM types).
  - Import cleanup: `src/bin/temple_hc.rs` no longer carries VM-only `std`/`temple_rt` imports; `temple_rt::protocol` is imported where it’s used (tests), and the VM gets its dependencies via `vm::prelude`.
  - Import cleanup: `src/bin/temple_hc.rs` no longer provides shared `std`/`temple_rt` imports for the remaining include fragments:
    - `src/bin/temple_hc/03_preprocess.rs` and `src/bin/temple_hc/04_cli.rs` now use fully-qualified `std::...` paths (avoids duplicate `use` names under `include!()`).
    - `src/bin/temple_hc/04_cli.rs` uses fully-qualified `temple_rt::rt::TempleRt`.
    - `src/bin/temple_hc/05_tests.rs` imports `temple_rt::rt::TempleRt` locally.
  - ✅ No `include!()` remain in `src/bin/temple_hc/vm/` (builtins are real submodules now).
  - ✅ Deleted legacy wrapper `src/bin/temple_hc/01_vm.rs` (it’s no longer included by `src/bin/temple_hc.rs`).
  - ✅ Verified: `cargo test -q` passes (2026-02-12).
  - ✅ Verified: `TEMPLE_GUI_TESTS=1 cargo test -q gui_smoke --test gui_smoke` passes (2026-02-12).
  - ✅ Verified: `packaging/bin/templelinux-publish-screenshots` succeeded (2026-02-12).
    - Index: https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/12/022330-3369838/index.html
    - Local output dir: `/tmp/templelinux-screenshots-0Fvcskiz` (temp dir)

## Key constraint: we currently use `include!` (legacy single-scope assumptions)
`src/bin/temple_hc.rs` still `include!()`s `00_frontend.rs`, `02_fmt.rs`, etc, and the VM code historically lived in that same scope. A lot of VM code assumes it can “see” frontend types and imports without explicit `use`.

To minimize churn, Phase 1 keeps the **single-module** model by splitting `01_vm.rs` into **include fragments**. Phase 2 starts adding real module boundaries while minimizing churn (a controlled `vm::prelude` import list + a small `pub(super)` surface).

## Phase 1 (recommended): include-split into >= 7 files (no behavior change)

### Proposed directory layout
Create a new folder:
- `src/bin/temple_hc/vm/`

Then (Phase 1) replace `src/bin/temple_hc/01_vm.rs` with a tiny wrapper (later deleted in Phase 2 once the VM is a real module):
- `include!("vm/mod.rs");`

And add `src/bin/temple_hc/vm/mod.rs` that includes the new fragments in a safe order.

**Implemented layout (now in tree):**
- `src/bin/temple_hc/vm/mod.rs` (includes fragments)
- `src/bin/temple_hc/vm/prelude.rs` (explicit VM import list for Phase 2)
- `src/bin/temple_hc/vm/00_values.rs`
- `src/bin/temple_hc/vm/01_env.rs`
- `src/bin/temple_hc/vm/02_ui_types.rs`
- `src/bin/temple_hc/vm/03_vm_struct.rs`
- `src/bin/temple_hc/vm/04_init.rs`
- `src/bin/temple_hc/vm/05_heap_doldoc_rng.rs`
- `src/bin/temple_hc/vm/06_linux_bridge.rs`
- `src/bin/temple_hc/vm/07_exec.rs`
- `src/bin/temple_hc/vm/08_eval.rs`
- `src/bin/temple_hc/vm/09_ui.rs`
- `src/bin/temple_hc/vm/10_text.rs`
- `src/bin/temple_hc/vm/11_call.rs`
- `src/bin/temple_hc/vm/builtins/mod.rs`
- `src/bin/temple_hc/vm/builtins/core.rs`
- `src/bin/temple_hc/vm/builtins/ui_input_sound.rs`
- `src/bin/temple_hc/vm/builtins/gfx.rs`
- `src/bin/temple_hc/vm/builtins/doc_fs_settings.rs`
- `src/bin/temple_hc/vm/builtins/linux.rs`

### File map (concrete)
This breaks `01_vm.rs` into **12+ files** (comfortably above the “>= 7 files” requirement) and keeps each area cohesive.

1) `src/bin/temple_hc/vm/00_values.rs`
- Move: `Obj`, `ArrayValue`, `Value` (+ `impl Value`), `VarType`, `ScalarKind` (+ `impl VarType`)
- Rationale: everything depends on `Value`; isolate it early.

2) `src/bin/temple_hc/vm/01_env.rs`
- Move: `Env` (+ `impl Env`), `EnvScopeGuard` (+ `Drop`), `ControlFlow`, `VmPanic`
- Rationale: execution/eval uses env + control flow constantly.

3) `src/bin/temple_hc/vm/02_ui_types.rs`
- Move: `TempleMsg`, `MenuAction`, `MenuItem`, `MenuGroup`, `MenuUnderlay`, `MenuState`
- Rationale: UI/menu plumbing is self-contained and shouldn’t sit in “VM core”.

4) `src/bin/temple_hc/vm/03_vm_struct.rs`
- Move: `struct Vm { ... }` (fields only)
- Rationale: keep the “shape” of the VM in one place; impls can be split freely.

5) `src/bin/temple_hc/vm/04_init.rs`
- Move methods: `Vm::new`, `compute_initial_cwd`, `define_sub`, `now_nanos`
- Also: (small utilities; exact grouping can shift during split)
- Rationale: initialization + small utilities; stable base for later splits.

6) `src/bin/temple_hc/vm/05_heap_doldoc_rng.rs`
- Move methods: `heap_*`, `alloc_class_value`, `read_cstr_lossy`, `load_doldoc_bin`, `set_seed`, `rand_next_u64`, `rand_i16`
- Rationale: memory model + DolDoc bins + RNG are “runtime substrate”.

7) `src/bin/temple_hc/vm/06_linux_bridge.rs`
- Move methods: `linux_run_allowlist`, `sway_workspace_number`, `maybe_auto_linux_ws`, `split_cmdline`
- Move methods: `resolve_linux_open_target`, `resolve_linux_open_target_temple_root_only`, `normalize_temple_path`,
  `resolve_temple_spec_read`, `resolve_temple_spec_write`, `resolve_temple_fs_target_read`, `resolve_temple_fs_target_write`
- Rationale: host OS integration is a separate concern (and easy to accidentally grow).

8) `src/bin/temple_hc/vm/07_exec.rs`
- Move methods: `exec_snippet`, `run`, `exec_stmts_with_goto`, `exec_block_unscoped`, `exec_block`,
  `exec_stmt`, `exec_switch_stmt`, `exec_switch_arm`
- Rationale: keep statement-level execution separate from expression eval and host APIs.

9) `src/bin/temple_hc/vm/08_eval.rs`
- Move methods: `eval_addr_of`, `eval_expr`, `eval_cmp_bool`, `eval_index`, `eval_bin`,
  `get_field`, `set_field`, `get_subint_view`, `assign_lhs`, `eval_int_expr_str`
- Move methods: `eval_arg_i64`, `eval_arg_f64`, `sizeof_expr`
- Move methods: `type_size_bytes`, `cast_int_bits`, `cast_raw_bits_to_value`,
  `is_text_font_expr`, `try_read_u64_from_lvalue_expr`, `read_u64_from_array`
- Move methods: init/default helpers: `is_class_value_type`, `default_value_for_type`,
  `eval_init_list_for_type`, `eval_init_expr_for_type`, `eval_class_init_list`, `eval_array_value`,
  `default_value_for_decl`, `eval_decl_value`
- Rationale: “evaluate HolyC AST” stays together; this is where most correctness bugs live.

10) `src/bin/temple_hc/vm/09_ui.rs`
- Move methods: ctrl + events + menu + overlays:
  - `ctrl_find_at`, `ctrl_call_left_click`, `ctrl_handle_left_button`
  - `poll_events`, `map_key_event_to_msg`, `scan_msg_mask`, `map_key_code`
  - `set_fs_cur_menu`, `parse_menu_spec`, `menu_action_from_args`, `menu_push`, `menu_pop`,
    `menu_item_label`, `menu_bar_hit`, `menu_dropdown_rect`, `menu_dropdown_hit`,
    `menu_capture_underlay`, `menu_restore_underlay`, `menu_set_open_group`,
    `menu_update_hover`, `menu_handle_left_click`, `render_menu_overlay`
  - `maybe_call_draw_it`, `maybe_draw_ctrls`, `maybe_draw_mouse_overlay`, `present_with_overlays`
- Rationale: this is “TempleOS UI behavior”; it should not be buried in the evaluator.

11) `src/bin/temple_hc/vm/10_text.rs`
- Move methods: `capture_push`, `newline`, `put_char`, `print_str`, `try_apply_doldoc_code`,
  `doldoc_color_name_to_idx`, `print_putchars`, `exec_print`
- Rationale: terminal/text/DolDoc output is its own subsystem.

12) `src/bin/temple_hc/vm/11_call.rs`
- Keep: `is_builtin`, `call` (but slim it down by delegating builtins to separate files below)
- Rationale: “call” is the choke-point; keep it small and readable.

13) Builtins split (implemented, coarse)
`Vm::call()` delegates builtins to `Vm::call_builtin(...)` (in `src/bin/temple_hc/vm/builtins/mod.rs`), and the bulk of the builtin code lives in:
- `src/bin/temple_hc/vm/builtins/core.rs`
- `src/bin/temple_hc/vm/builtins/ui_input_sound.rs`
- `src/bin/temple_hc/vm/builtins/gfx.rs`
- `src/bin/temple_hc/vm/builtins/doc_fs_settings.rs`
- `src/bin/temple_hc/vm/builtins/linux.rs`

This gets rid of the ~3k-LOC `match` inside `Vm::call()` while keeping behavior identical.

Future refinement (optional): split those chunk files into more granular per-area handlers (memory/fs/strings/time_math/gfx/input_menu/window_doc/sound/linux/registry_settings) if we want even smaller files and clearer ownership.

With this split, `vm/11_call.rs` is now:
- “user-defined function” call path
- + `call_builtin(...)` delegation
- + the fallback “unknown function” error

## Extraction order (minimize breakage)
Do this as a sequence of mechanical moves; after each step: build + tests.

1) Create `vm/` folder and `vm/mod.rs` (empty), turn `01_vm.rs` into wrapper include.
2) Move type definitions first (`00_values.rs`, `01_env.rs`, `02_ui_types.rs`, `03_vm_struct.rs`).
3) Move substrate methods (`04_init.rs`, `05_heap_doldoc_rng.rs`, `06_linux_bridge.rs`).
4) Move interpreter (`07_exec.rs`, `08_eval.rs`).
5) Move UI/event/menu/text (`09_ui.rs`, `10_text.rs`).
6) Split `call()`:
   - First extract “user-defined function call” block (keep behavior same).
   - Then peel off builtins in batches into `builtins/*` and wire through `builtins::call_builtin`.

## Verification checklist (each milestone)
- `cargo test -q`
- `TEMPLE_GUI_TESTS=1 cargo test -q gui_smoke --test gui_smoke`
- Run the screenshot publisher once to ensure UI regressions are obvious:
  - `packaging/bin/templelinux-publish-screenshots` (uses Xvfb + uploads gallery)

## Acceptance criteria
- `src/bin/temple_hc/01_vm.rs` is eliminated (it may temporarily shrink to a thin wrapper (~1–20 LOC) during the include-split transition).
- VM implementation split into **>= 7** focused files (target: 12–20 files, none > ~1,200 LOC).
- No functional regressions: vendored TempleOS demos still run; existing tests + GUI goldens pass.

## Phase 2 (optional later): convert include fragments into real Rust modules
Once the include-split stabilizes, we can convert to `mod vm;` and real modules to get:
- Real privacy boundaries (`pub(super)` instead of “everything in one scope”).
- Faster incremental compilation.
- Clearer ownership for future contributors.

This is optional because it’s higher-churn (imports + visibility), but it’s the long-term “clean” end state.

Progress so far:
- ✅ Builtins are now a real module boundary (`src/bin/temple_hc/vm/mod.rs` declares `mod builtins;`).
- ✅ `00_values.rs` is now a real `vm::values` module (via `#[path = "00_values.rs"] mod values;`), with `Value` re-exported at the VM root.
- ✅ `01_env.rs` is now a real `vm::env` module (via `#[path = "01_env.rs"] mod env;`), with `Env`/`ControlFlow`/`EnvScopeGuard` imported into the VM root.
- ✅ `02_ui_types.rs` is now a real `vm::ui_types` module (via `#[path = "02_ui_types.rs"] mod ui_types;`), with menu/UI types imported at the VM root.
- ✅ `03_vm_struct.rs` is now a real `vm::vm_struct` module (via `#[path = "03_vm_struct.rs"] mod vm_struct;`), with `Vm` re-exported at the VM root.
- ✅ `04_init.rs` is now a real `vm::init` module (via `#[path = "04_init.rs"] mod init;`).
- ✅ `05_heap_doldoc_rng.rs` is now a real `vm::heap_doldoc_rng` module (via `#[path = "05_heap_doldoc_rng.rs"] mod heap_doldoc_rng;`).
- ✅ `06_linux_bridge.rs` is now a real `vm::linux_bridge` module (via `#[path = "06_linux_bridge.rs"] mod linux_bridge;`).
- ✅ `07_exec.rs` is now a real `vm::exec` module (via `#[path = "07_exec.rs"] mod exec;`).
- ✅ `08_eval.rs` is now a real `vm::eval` module (via `#[path = "08_eval.rs"] mod eval;`).
- ✅ `09_ui.rs` is now a real `vm::ui` module (via `#[path = "09_ui.rs"] mod ui;`).
- ✅ `10_text.rs` is now a real `vm::text` module (via `#[path = "10_text.rs"] mod text;`).
- ✅ `11_call.rs` is now a real `vm::call` module (via `#[path = "11_call.rs"] mod call;`).
- ✅ VM is now behind a real `crate::vm` module boundary (`src/bin/temple_hc.rs` declares `#[path = "temple_hc/vm/mod.rs"] mod vm;`).
- ✅ Removed `use super::*;` from the VM root and replaced it with a controlled `vm::prelude` import list.
- ✅ Removed `use super::*;` from `src/bin/temple_hc/vm/builtins/mod.rs` (explicit imports via `super::prelude` + selected VM items).

Next steps (suggested order):
1) ✅ Convert `src/bin/temple_hc/vm/builtins/mod.rs` from `include!(...)` to real submodules (`mod core; mod gfx; ...`) and fix `pub(super)` visibility fallout.
2) ✅ Continue replacing “implicit” imports from `src/bin/temple_hc.rs` with explicit per-module paths/imports (2026-02-12).
3) ✅ Remove legacy wrapper `src/bin/temple_hc/01_vm.rs` once Phase 2 stabilizes (it’s no longer included by `src/bin/temple_hc.rs`).
4) ✅ Run GUI smoke tests: `TEMPLE_GUI_TESTS=1 cargo test -q gui_smoke --test gui_smoke` (2026-02-12).
5) ✅ Run screenshot publisher: `packaging/bin/templelinux-publish-screenshots` (2026-02-12).
   - Index: https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/12/022330-3369838/index.html

## Phase 3 (optional later): convert `src/bin/temple_hc.rs` include fragments into real modules
Status: ✅ Completed (2026-02-12).

Before this phase, `src/bin/temple_hc.rs` was mostly “frontend/compiler + CLI glue” and still used
`include!("temple_hc/*.rs")` for:
- `00_frontend.rs` (lexer/parser/AST)
- `02_fmt.rs` (Temple format-string engine)
- `03_preprocess.rs` (TempleOS-ish preprocessor + include resolver)
- `04_cli.rs` (binary entrypoint)
- `05_tests.rs` (tests)

This was *fine* to keep as-is, but the `include!()` approach had a few persistent downsides:
- Shared-scope `use` collisions (`E0252`) make imports awkward (we currently rely on fully-qualified
  `std::...` paths in `03_preprocess.rs` / `04_cli.rs` to avoid that).
- Everything lives in one giant module namespace, which makes “who owns this?” unclear.
- Privacy is all-or-nothing; it’s hard to express “used by preprocess and VM, but not CLI”.

If we decide to pay the churn cost, converting these fragments into real modules would let us:
- Use normal `use` imports again (and likely undo the fully-qualified `std::...` workaround).
- Tighten the visible surface between frontend/preprocess/fmt/cli/tests.
- Make it easier to reason about and test subsystems in isolation.

Implementation notes:
- `src/bin/temple_hc.rs` now declares `#[path = "temple_hc/mod.rs"] mod hc;` and the crate-root
  `fn main()` calls `hc::run()`.
- `src/bin/temple_hc/mod.rs` is the module root for the compiler frontend (former `00_frontend.rs`),
  and declares `fmt`/`preprocess`/`cli`/`vm` as real submodules via `#[path = "..."] mod ...;`.
- `fmt` + `preprocess` expose a small `pub(super)` surface so sibling modules can share the handful of
  entrypoints/types they need (while keeping AST/Program privacy intact for `vm/`).

Verification stays the same:
- `cargo test -q`
- `TEMPLE_GUI_TESTS=1 cargo test -q gui_smoke --test gui_smoke`

Suggested incremental checklist (keep this section updated as work lands):
- [x] Create `src/bin/temple_hc/mod.rs` and move `00_frontend.rs` contents into the module root
      (so `vm/` remains a descendant and can keep accessing “private” AST/Program fields).
- [x] Rewrite `src/bin/temple_hc.rs` to `#[path = "temple_hc/mod.rs"] mod hc;` and a tiny crate-root
      `fn main()` that calls `hc::run()`.
      - Note: `mod temple_hc;` conflicts with the bin crate root name (`src/bin/temple_hc.rs`) and
        triggers `E0761`, so we use a distinct module name (`hc`) + `#[path]`.
- [x] Convert `02_fmt.rs`, `03_preprocess.rs`, `04_cli.rs` into real submodules of `hc` using
      `#[path = "..."] mod ...;` (no more crate-root `include!()`).
- [x] Add a small, explicit `pub(super)` surface between `fmt`/`preprocess`/`cli` modules (only what
      siblings need).
- [x] Update `src/bin/temple_hc/vm/prelude.rs` to import from `super::super::{...}` + `fmt`/`preprocess`
      instead of `crate::{...}`.
- [x] Fix the HolyC tests module wiring (keep `05_tests.rs` mechanical; import what it needs into an
      outer wrapper module rather than reindenting 1.6k LOC).
- [x] Verify: `cargo test -q` (2026-02-12)
- [x] Verify: `TEMPLE_GUI_TESTS=1 cargo test -q gui_smoke --test gui_smoke` (2026-02-12)

## Phase 4 (optional): post-module cleanup (imports + ergonomics)
Status: ✅ Completed (2026-02-12).

Now that the `include!()`-based shared-scope problem is gone (Phase 3), we can undo the
“fully-qualified `std::...` everywhere” workaround in the remaining frontend modules.

Checklist:
- [x] Replace fully-qualified `std::...` paths with normal `use std::...` imports in
      `src/bin/temple_hc/03_preprocess.rs` (no behavior changes).
- [x] Replace fully-qualified `std::...` paths with normal `use std::...` imports in
      `src/bin/temple_hc/04_cli.rs` (no behavior changes).
- [x] Verify: `cargo test -q` (2026-02-12)
- [x] Verify: `TEMPLE_GUI_TESTS=1 cargo test -q gui_smoke --test gui_smoke` (2026-02-12)
