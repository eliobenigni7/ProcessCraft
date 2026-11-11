# ProcessCraft M0 evidence

Date: November 11, 2026. Scope: M0 implementation evidence, with results and qualification limits recorded per gate below. Later milestones and aggregate M0 acceptance must not be inferred from one gate. No Git repository exists in this checkout; no commit was created.

## M0 acceptance

**Tasks 01–04 are implemented and passed their specification and code-quality reviews within the boundaries below.** The later editor/workbench is not implemented.

A final independent cross-package review also approved M0, including the actual sources, dependency boundaries and retained independent-consumer reports. Historical task snapshots below are labeled where their package/test counts differ from the final aggregate.

| Gate | Result | Qualified boundary |
|---|---|---|
| G1 | PASS | Native Windows shell at 96 DPI, one compatible egui/Craft family, Light/Dark, real pointer/keyboard checks and typed IDs |
| G2 | PASS within stated scope | Raw-byte replacement on tested fixed-local NTFS fixtures, interprocess ownership, staged access restrictions and explicit outcomes; metadata/path refusals and unqualified cases remain below |
| G3 | PASS within stated scope | Bounded read-only original-source preservation; five exported synthetic fixtures reopened in bpmn-moddle 10.3.1 with zero warnings and exact source hashes |
| G4 | PASS within stated scope | One embedded-font layout/path stream, native PNG plus independently rendered SVG/PDF at 100%/200%; PDF is evidence-only and outlined |

The latest `cargo test --workspace --all-targets --locked --offline` passed **40 tests**: 3 core, 1 storage unit, 11 BPMN, 17 storage integration (including its child helper), 7 render integration and 1 PDF example test. Workspace formatting and locked/offline check passed. Root also ran `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` successfully and independently verified all **11** fixture/font/corpus hashes. Metadata confirms exactly four M0 packages, no SQLite dependency, no GUI/I/O dependency in core and no I/O/render dependency on each other.

M0 does not qualify editable BPMN imports, a native process-editing canvas, project ZIP/JSON save integration, arbitrary Windows metadata/filesystems, product PDF text extraction/embedding/pagination, other platforms, a full accessibility pass or M1 performance targets. The raster proof deliberately limits allocation to **64 MiB RGBA / 16,777,216 pixels**; the architecture's 32-megapixel product target remains a later qualification. Continue at implementation Task 05 for the domain model.

## Task 01: native shell and dependency compatibility

**G1 status: PASS on the Windows configuration below.** Build, automated checks and independent live Windows verification passed after the panel-fill fix. Task 01 is complete; the remaining M0 gates are separate tasks.

### Machine and toolchain

- Windows 11 Pro, version 10.0.26300, build 26300; native target `x86_64-pc-windows-msvc`.
- AMD Ryzen 5 5600X, approximately 15.9 GiB physical RAM.
- Initially installed default: `stable-x86_64-pc-windows-msvc`, rustc `1.98.1 (48a229cea 2026-09-01)`, cargo `1.98.1 (797e8a9bc 2026-08-05)`.
- Installed the matching named toolchain with `rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy`. `rust-toolchain.toml` now pins `1.98.1`; Cargo.lock records the resolved dependencies.
- Application-window DPI: 96, corresponding to 100% display scaling. No other OS or display configuration is qualified.

### Resolved family and reuse decision

Inspected the pinned [Craft workspace manifest](https://github.com/storytold/craft-libs/blob/acc262e69cd7a048bba323c63a764e310fa0d500/Cargo.toml), [craft-ui manifest](https://github.com/storytold/craft-libs/blob/acc262e69cd7a048bba323c63a764e310fa0d500/crates/ui/Cargo.toml) and [README](https://github.com/storytold/craft-libs/blob/acc262e69cd7a048bba323c63a764e310fa0d500/crates/ui/README.md). Craft requires Rust >=1.95 and egui 0.37, and pins its egui and winit forks. Reused those exact revisions rather than combining upstream egui types with fork types. The narrow `craft_ui::buttons::IconButton` toggles the visibility of real core ProjectId/DocumentId values. Its accessible label and adjacent visible text say “Show identifiers” / “Hide identifiers.” The app supplies the icon painter and theme. Compatibility succeeded; upstream fallback was unnecessary.

| Dependency family | Exact revision / version | Selected application features | Declared license |
|---|---|---|---|
| craft-ui, craft-fonts-build, craft-i18n | craft-libs `acc262e69cd7a048bba323c63a764e310fa0d500`, 0.1.0 | No Craft features; the latter two are craft-ui transitive dependencies | MIT OR Apache-2.0 |
| eframe | egui fork `6d518fc799681571963842f95b55a6b68431b15a`, 0.37.0 | `accesskit`, `default_fonts`, `glow`; defaults disabled | MIT OR Apache-2.0 |
| egui, egui-winit, egui_glow, ecolor, emath, epaint | Same egui revision, all 0.37.0 | Host-resolved features listed below | MIT OR Apache-2.0 |
| epaint_default_fonts | Same egui revision, 0.37.0 | Bundled fonts | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 |
| winit | `a517b3846ee8457d84ee4975110bfda6797c44dc`, 0.30.13 | Host-resolved features listed below | Apache-2.0 |
| uuid | crates.io 1.29.2 | `std`, `v4` (enables `rng`); defaults disabled in core | Apache-2.0 OR MIT |

One egui/epaint/emath/ecolor family resolved. `cargo tree -d` reports unrelated duplicates: log 0.4.35 has separate host/build feature contexts; miniz_oxide 0.8.9/0.9.1, syn 2.0.119/3.0.10 and windows-sys 0.52.0/0.61.2 are transitive versions. No duplicate egui family was observed. OpenGL/glow is the selected UI backend; this is not a BPMN rendering decision. The fork itself brings vello_cpu transitively through epaint; ProcessCraft adds no renderer abstraction or renderer package here.

Core's entire host dependency tree is `uuid -> getrandom -> cfg-if`. It has no GUI, filesystem, serialization, database, or async dependency. IDs use private UUID fields, typed ProjectId/DocumentId wrappers, non-nil parsing, and UUID v4 generation. The desktop stores those typed IDs and displays their values on request; these in-memory session identities are not a persistent project implementation.

At the Task 01 snapshot there were exactly two workspace packages; the final M0 workspace has four. The English window has project/document information, identifier visibility and Light/Dark selection. No editing, opening, saving, native package format or fake model/save controls are included. Appearance and visibility are transient presentation state.

### Verification

| Command / check | Observed result |
|---|---|
| First `cargo test -p processcraft-core` before ID implementation | Expected RED, exit 1 / E0432: missing `ProjectId`, `DocumentId`, `InvalidId` exports |
| `cargo test -p processcraft-core` after implementation | PASS: 3 tests, 0 failures; 0 doc tests |
| `cargo check --workspace` | PASS, both workspace packages and shared Craft control compile |
| `cargo check --workspace --locked --offline` | PASS after dependencies were fetched; offline application builds use the lockfile |
| `cargo tree -d` | PASS; duplicates assessed above |
| `cargo tree -e features` | PASS; actual host features captured and inventoried below |
| `cargo metadata --format-version 1 --no-deps` | PASS; exactly core and desktop members |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS: 3 core tests; 0 desktop tests; 0 doc tests |
| `cargo run -p processcraft` | PASS: built and launched the native executable; independent interactive results below |

The regressions cover both identity types: malformed/nil rejection, parse/display identity preservation and generated valid/distinct values. No GUI unit test is substituted for the live Windows check.

Raw generated outputs are under ignored `target/m0-task01/`: `duplicates.txt`, `features.txt`, `workspace-metadata.json`, `resolved-metadata.json`, `licenses-features.csv`. Regenerate the first three with the corresponding commands above, redirecting stdout. `resolved-metadata.json` uses `cargo metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc` and supplies declared license/source fields. The inventory below takes actual host feature edges from `cargo tree --prefix none --format '{p}|{f}' -e normal,build`, merges features for repeated name/version contexts, and joins licenses to metadata. This avoids mistaking metadata's broader resolver feature set for the Windows compilation feature set. Cargo metadata can list features such as egui `web_file` that are not enabled in the host tree.

License values below are observed package metadata, including non-code font/data terms. They are an inventory, not a completed distribution-license review. Local ProcessCraft packages have no selected distribution license yet.

### Interactive Windows check

The root reviewer verified the actual native ProcessCraft window (PID 10292 after the fix), measured its DPI, used Windows UIAutomation and real keyboard input, and inspected screen captures. Results:

- Native startup and window focus passed at 96 DPI / 100% scale. The Craft identifier control reported `HasKeyboardFocus = true`.
- Windows UIAutomation activated the Craft “Show identifiers” control; its label changed to “Hide identifiers” and both typed core IDs became visible. The values stayed unchanged across Light/Dark switching.
- Actual pointer activation passed too: after activating the application, the reviewer positioned the cursor at the Craft button center (154, 322) and sent Windows left-button down/up input. The state changed from visible IDs / “Hide identifiers” to “Show identifiers”; Windows UIAutomation confirmed that the ID labels disappeared.
- Both theme selectors responded through Windows UIAutomation. Then real Tab input moved focus from Dark to Light; Space selected Light and changed the surface.
- Light surface `#f7f8fa` and Dark surface `#191d24` rendered with readable labels. The in-memory limitation was visible. No save or model controls were present.
- Screenshots captured at local times 13:06:52 (Light), 13:06:59 (Dark), and 13:07:17 (keyboard-selected Light). Local copies: [Light](../../target/m0-task01/native-light.png), [Dark](../../target/m0-task01/native-dark.png), [Keyboard-selected Light](../../target/m0-task01/native-keyboard-light.png). Originals were `D:/Temp/codex-shot-2026-11-11_13-06-52.png`, `D:/Temp/codex-shot-2026-11-11_13-06-59.png`, and `D:/Temp/codex-shot-2026-11-11_13-07-17.png`.

This is a focused Windows smoke check covering Windows UIAutomation, actual pointer activation and keyboard theme selection. Full accessibility qualification, additional DPI settings, other platforms, persistent state and the M1 editor are not covered.

The first independent screenshot exposed a real visual defect: this eframe fork's root `App::ui` does not paint `visuals.panel_fill`, leaving a black background in both themes and unreadable Light text. Fixed by painting the root panel fill before content, rebuilt and relaunched. Formatting and desktop all-target Clippy passed after the fix. The final independent screenshots and interaction checks above passed; the initial defective view is not acceptance evidence.

## Resolved Windows normal/build dependency inventory

The following generated table covers third-party packages reachable in the host tree. Feature sets are unions across host/build contexts for the same name/version; empty means no crate features. The lockfile remains the complete cross-target resolution.

| Package | Version | Host features | Declared license |
|---|---|---|---|
| accesskit_consumer | 0.39.2 |  | MIT OR Apache-2.0 |
| accesskit_windows | 0.35.2 |  | MIT OR Apache-2.0 |
| accesskit_winit | 0.34.2 | accesskit_unix, async-io, default, rwh_06 | Apache-2.0 |
| accesskit | 0.25.2 |  | MIT OR Apache-2.0 |
| adler2 | 2.0.1 |  | 0BSD OR MIT OR Apache-2.0 |
| ahash | 0.8.12 | no-rng, std | MIT OR Apache-2.0 |
| arrayvec | 0.7.8 |  | MIT OR Apache-2.0 |
| autocfg | 1.5.1 |  | Apache-2.0 OR MIT |
| bitflags | 2.13.2 |  | MIT OR Apache-2.0 |
| bytemuck_derive | 1.12.1 |  | Zlib OR Apache-2.0 OR MIT |
| bytemuck | 1.25.2 | bytemuck_derive, derive, extern_crate_alloc, min_const_generics | Zlib OR Apache-2.0 OR MIT |
| byteorder-lite | 0.1.0 | default, std | Unlicense OR MIT |
| cfg_aliases | 0.2.2 |  | MIT |
| cfg-if | 1.0.5 |  | MIT OR Apache-2.0 |
| cobs | 0.3.0 |  | MIT OR Apache-2.0 |
| color | 0.3.3 | bytemuck, std | Apache-2.0 OR MIT |
| craft-fonts-build | 0.1.0 |  | MIT OR Apache-2.0 |
| craft-i18n | 0.1.0 |  | MIT OR Apache-2.0 |
| craft-ui | 0.1.0 |  | MIT OR Apache-2.0 |
| crc32fast | 1.5.5 | default, std | MIT OR Apache-2.0 |
| cursor-icon | 1.2.0 | alloc, default, std | MIT OR Apache-2.0 OR Zlib |
| displaydoc | 0.2.7 |  | MIT OR Apache-2.0 |
| document-features | 0.2.12 | default | MIT OR Apache-2.0 |
| dpi | 0.1.2 | default, std | Apache-2.0 AND MIT |
| ecolor | 0.37.0 | bytemuck | MIT OR Apache-2.0 |
| eframe | 0.37.0 | accesskit, default_fonts, glow | MIT OR Apache-2.0 |
| egui_glow | 0.37.0 |  | MIT OR Apache-2.0 |
| egui-winit | 0.37.0 | accesskit | MIT OR Apache-2.0 |
| egui | 0.37.0 | bytemuck, default_fonts | MIT OR Apache-2.0 |
| either | 1.19.0 | std, use_std | MIT OR Apache-2.0 |
| emath | 0.37.0 | bytemuck | MIT OR Apache-2.0 |
| epaint_default_fonts | 0.37.0 | default | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 |
| epaint | 0.37.0 | bytemuck, default_fonts, epaint_default_fonts | MIT OR Apache-2.0 |
| euclid | 0.22.14 |  | MIT OR Apache-2.0 |
| fdeflate | 0.3.7 |  | MIT OR Apache-2.0 |
| fearless_simd | 0.4.1 | std | Apache-2.0 OR MIT |
| fixed_decimal | 0.7.3 |  | Unicode-3.0 |
| flate2 | 1.1.11 | any_impl, default, miniz_oxide, runtime_detection, rust_backend | MIT OR Apache-2.0 |
| foldhash | 0.2.0 |  | Zlib |
| font-types | 0.12.7 | bytemuck, std | MIT OR Apache-2.0 |
| getrandom | 0.4.3 |  | MIT OR Apache-2.0 |
| gl_generator | 0.14.0 |  | Apache-2.0 |
| glow | 0.17.0 |  | MIT OR Apache-2.0 OR Zlib |
| glutin_egl_sys | 0.7.1 |  | Apache-2.0 |
| glutin_wgl_sys | 0.6.1 |  | Apache-2.0 |
| glutin-winit | 0.5.0 | egl, wgl | MIT |
| glutin | 0.32.3 | egl, glutin_egl_sys, glutin_wgl_sys, libloading, wgl, windows-sys | Apache-2.0 |
| guillotiere | 0.7.0 |  | MIT/Apache-2.0 |
| harfrust | 0.12.0 | default, std | MIT |
| hashbrown | 0.17.1 | default-hasher | MIT OR Apache-2.0 |
| icu_collections | 2.3.0 |  | Unicode-3.0 |
| icu_decimal | 2.3.0 | alloc, serde | Unicode-3.0 |
| icu_locale_core | 2.3.0 | alloc, zerovec | Unicode-3.0 |
| icu_locale_fallback_data | 2.3.0 |  | Unicode-3.0 |
| icu_locale_fallback | 2.3.0 | compiled_data | Unicode-3.0 |
| icu_normalizer_data | 2.3.0 |  | Unicode-3.0 |
| icu_normalizer | 2.3.0 | compiled_data, default, utf16_iter, utf8_iter | Unicode-3.0 |
| icu_plurals | 2.3.0 | serde | Unicode-3.0 |
| icu_properties_data | 2.3.0 |  | Unicode-3.0 |
| icu_properties | 2.3.0 | compiled_data, default | Unicode-3.0 |
| icu_provider_adapters | 2.3.0 |  | Unicode-3.0 |
| icu_provider_blob | 2.3.0 |  | Unicode-3.0 |
| icu_provider | 2.3.1 | alloc, baked, deserialize_postcard_1, serde, sync | Unicode-3.0 |
| image | 0.25.10 | png | MIT OR Apache-2.0 |
| itertools | 0.15.0 | default, use_alloc, use_std | MIT OR Apache-2.0 |
| khronos_api | 3.1.0 |  | Apache-2.0 |
| kurbo | 0.13.1 | std | Apache-2.0 OR MIT |
| libloading | 0.8.9 |  | ISC |
| linebender_resource_handle | 0.1.1 | std | Apache-2.0 OR MIT |
| litemap | 0.8.3 | alloc, serde | Unicode-3.0 |
| litrs | 1.0.1 |  | MIT OR Apache-2.0 |
| lock_api | 0.4.14 | atomic_usize, default | MIT OR Apache-2.0 |
| log | 0.4.35 | alloc, std | MIT OR Apache-2.0 |
| miniz_oxide | 0.8.9 | default, simd, simd-adler32, with-alloc | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | default, simd, simd-adler32, with-alloc | MIT OR Zlib OR Apache-2.0 |
| moxcms | 0.8.1 | avx, avx_luts, avx_shaper_fixed_point_paths, avx_shaper_paths, default, lut, neon, neon_luts, neon_shaper_fixed_point_paths, neon_shaper_paths, sse, sse_luts, sse_shaper_fixed_point_paths, sse_shaper_paths | BSD-3-Clause OR Apache-2.0 |
| nohash-hasher | 0.2.0 | default, std | Apache-2.0 OR MIT |
| num-traits | 0.2.19 | default, std | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | alloc, default, race, std | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 |  | MIT OR Apache-2.0 |
| parking_lot | 0.12.5 | default | MIT OR Apache-2.0 |
| peniko | 0.6.1 | bytemuck, std | Apache-2.0 OR MIT |
| pin-project-lite | 0.2.17 |  | Apache-2.0 OR MIT |
| png | 0.18.1 |  | MIT OR Apache-2.0 |
| polycool | 0.4.0 | std | MIT OR Apache-2.0 |
| postcard | 1.1.3 |  | MIT OR Apache-2.0 |
| potential_utf | 0.1.6 | alloc, writeable, zerovec | Unicode-3.0 |
| proc-macro2 | 1.0.107 | default, proc-macro | MIT OR Apache-2.0 |
| profiling | 1.0.18 |  | MIT OR Apache-2.0 |
| pxfm | 0.1.30 |  | BSD-3-Clause OR Apache-2.0 |
| quote | 1.0.47 | default, proc-macro | MIT OR Apache-2.0 |
| raw-window-handle | 0.6.2 | alloc, std | MIT OR Apache-2.0 OR Zlib |
| read-fonts | 0.41.0 | experimental_font_api, std | MIT OR Apache-2.0 |
| scopeguard | 1.2.0 |  | MIT OR Apache-2.0 |
| self_cell | 1.3.0 |  | Apache-2.0 OR GPL-2.0-only |
| serde_core | 1.0.229 | alloc, result | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | default | MIT OR Apache-2.0 |
| serde | 1.0.229 | alloc, derive, serde_derive | MIT OR Apache-2.0 |
| simd-adler32 | 0.3.10 | const-generics, default, std | MIT |
| skrifa | 0.44.0 | autohint_shaping, std | MIT OR Apache-2.0 |
| smallvec | 1.16.5 | const_generics, const_new | MIT OR Apache-2.0 |
| smol_str | 0.2.2 | default, std | MIT OR Apache-2.0 |
| stable_deref_trait | 1.2.1 | alloc | MIT OR Apache-2.0 |
| static_assertions | 1.1.0 |  | MIT OR Apache-2.0 |
| syn | 2.0.119 | clone-impls, full, parsing, printing, proc-macro | MIT OR Apache-2.0 |
| syn | 3.0.10 | clone-impls, default, derive, extra-traits, fold, parsing, printing, proc-macro, visit | MIT OR Apache-2.0 |
| synstructure | 0.14.0 | default, proc-macro | MIT |
| sys-locale | 0.3.2 |  | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 |  | MIT OR Apache-2.0 |
| thiserror | 2.0.21 |  | MIT OR Apache-2.0 |
| tinystr | 0.8.5 | alloc, zerovec | Unicode-3.0 |
| tracing-core | 0.1.36 |  | MIT |
| tracing | 0.1.44 |  | MIT |
| unicode-bidi | 0.3.18 | default, hardcoded-data, std | MIT OR Apache-2.0 |
| unicode-general-category | 1.1.0 |  | Apache-2.0 |
| unicode-ident | 1.0.27 |  | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-properties | 0.1.5 | emoji | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-segmentation | 1.13.3 |  | MIT OR Apache-2.0 |
| utf_types | 1.0.0 |  | Apache-2.0 OR MIT |
| utf16_iter | 1.1.0 |  | Apache-2.0 OR MIT |
| utf8_iter | 1.1.0 |  | Apache-2.0 OR MIT |
| uuid | 1.29.2 | rng, std, v4 | Apache-2.0 OR MIT |
| vello_common | 0.2.0 | std | Apache-2.0 OR MIT |
| vello_cpu | 0.2.0 | f32_pipeline, std, u8_pipeline | Apache-2.0 OR MIT |
| version_check | 0.9.5 |  | MIT/Apache-2.0 |
| web-time | 1.1.0 |  | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.52.6 |  | MIT OR Apache-2.0 |
| windows-collections | 0.3.2 | std | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | default, std | MIT OR Apache-2.0 |
| windows-future | 0.3.2 | std | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 |  | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 |  | MIT OR Apache-2.0 |
| windows-link | 0.2.1 |  | MIT OR Apache-2.0 |
| windows-numerics | 0.3.1 | std | MIT OR Apache-2.0 |
| windows-registry | 0.6.1 | default, std | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | std | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | std | MIT OR Apache-2.0 |
| windows-sys | 0.52.0 | default, Win32, Win32_Devices, Win32_Devices_HumanInterfaceDevice, Win32_Foundation, Win32_Globalization, Win32_Graphics, Win32_Graphics_Dwm, Win32_Graphics_Gdi, Win32_Graphics_OpenGL, Win32_Media, Win32_Security, Win32_System, Win32_System_Com, Win32_System_Com_StructuredStorage, Win32_System_LibraryLoader, Win32_System_Ole, Win32_System_SystemInformation, Win32_System_SystemServices, Win32_System_Threading, Win32_System_WindowsProgramming, Win32_UI, Win32_UI_Accessibility, Win32_UI_Controls, Win32_UI_HiDpi, Win32_UI_Input, Win32_UI_Input_Ime, Win32_UI_Input_KeyboardAndMouse, Win32_UI_Input_Pointer, Win32_UI_Input_Touch, Win32_UI_Shell, Win32_UI_TextServices, Win32_UI_WindowsAndMessaging | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | default, Win32, Win32_Foundation, Win32_Graphics, Win32_Graphics_Dwm, Win32_System, Win32_System_Com, Win32_UI, Win32_UI_Input, Win32_UI_Input_KeyboardAndMouse, Win32_UI_Shell, Win32_UI_WindowsAndMessaging | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 |  | MIT OR Apache-2.0 |
| windows-threading | 0.2.1 |  | MIT OR Apache-2.0 |
| windows | 0.62.2 | default, std, Win32, Win32_Foundation, Win32_Globalization, Win32_Graphics, Win32_Graphics_Gdi, Win32_System, Win32_System_Com, Win32_System_LibraryLoader, Win32_System_Ole, Win32_System_Variant, Win32_UI, Win32_UI_Accessibility, Win32_UI_Input, Win32_UI_Input_KeyboardAndMouse, Win32_UI_WindowsAndMessaging | MIT OR Apache-2.0 |
| winit | 0.30.13 | ahash, bytemuck, memmap2, percent-encoding, rwh_06, sctk, wayland, wayland-backend, wayland-client, wayland-protocols, wayland-protocols-plasma, x11, x11-dl, x11rb | Apache-2.0 |
| write16 | 1.0.0 | alloc | Apache-2.0 OR MIT |
| writeable | 0.6.4 | alloc, default | Unicode-3.0 |
| xml-rs | 0.8.29 |  | MIT |
| yoke-derive | 0.8.4 |  | Unicode-3.0 |
| yoke | 0.8.4 | alloc, derive, serde, zerofrom | Unicode-3.0 |
| zerocopy | 0.8.63 | simd | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerofrom-derive | 0.1.8 |  | Unicode-3.0 |
| zerofrom | 0.1.8 | alloc, derive | Unicode-3.0 |
| zerotrie | 0.2.5 | alloc, litemap, serde, yoke, zerofrom, zerovec | Unicode-3.0 |
| zerovec-derive | 0.11.7 |  | Unicode-3.0 |
| zerovec | 0.11.9 | alloc, derive, serde, yoke | Unicode-3.0 |

## Task 02: local file boundary and ownership

**G2: PASS for the scoped ordinary-local-NTFS fixture boundary; independent specification and code-quality reviews approved.** This is a raw-byte storage spike, with no project schema, GUI save integration, recovery, or automatic artifact sweeping. The qualified fixtures use ordinary files on the machine's fixed local NTFS volumes (D: temporary storage; E: workspace). It is not a claim of transparent replacement for arbitrary Windows metadata or arbitrary filesystem locations. The remaining metadata/location qualification limits below remain in force.

### Candidate and adapter decision

Read the pinned Craft Atomic README and `crates/atomic/src/lib.rs` at `acc262e69cd7a048bba323c63a764e310fa0d500` in Cargo's fetched checkout. Its `RealFs::rename` delegates to `std::fs::rename`, its interface returns `io::Result`, its symlink resolution can move beyond a selected path, and its documented replacement loses ACLs, ownership, streams and hard links. It was evaluated and not adopted for this Windows boundary. There is no unused Craft Atomic dependency.

The narrowly scoped alternative uses Win32 `ReplaceFileW` with a unique sibling backup and no flags that ignore ACL/merge errors. New outputs use `MoveFileExW` without replacement flags. [Microsoft's ReplaceFileW contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew) documents preservation of DACLs, named streams, creation time and certain other metadata, as well as errors that can occur after names have changed. The adapter therefore classifies an error as `NotCommitted` only if the original content remains at the target, the full new stage remains, and no backup appeared; otherwise it returns `Indeterminate`, retains artifacts, and blocks blind retry in that session. A known successful replacement followed by hook/cleanup trouble returns `Committed` with a warning.

`processcraft-io` uses a package-level `unsafe_code = "deny"`; only `save::windows` locally permits unsafe OS calls, with safety comments for every FFI block. All other packages retain the workspace's `forbid`. This is the architecture section 18 platform-adapter exception, reviewed at the implementation boundary rather than a workspace-wide relaxation. The only added dependency is `windows-sys = 0.61.2` (MIT OR Apache-2.0), with `Win32_Storage_FileSystem`, `Win32_Foundation`, `Win32_Security` and their feature ancestors; UUID reuses the existing workspace dependency. The package has no core, GUI, async or database dependency.

### Implemented boundary

- Explicit `FileGrant` capabilities carry read-only/write authority supplied by a trusted entry point. A read-only grant does not create a target, sidecar, stage or backup. Document content cannot grant access by itself.
- Grants reject UNC/device/ADS paths, dot traversal, trailing-dot/space and reserved Win32 filename aliases. Each ancestor is opened with `OPEN_REPARSE_POINT` and without share-delete; reparse points are refused. Existing target names resolve from the validated file handle to unify case and 8.3 aliases. The grant and writer retain directory handles. Only fixed local NTFS is enabled; other operating systems return `Unsupported` and have not been tested.
- Writer sessions keep a stable sibling `.lock` handle and `File::try_lock` ownership for their lifetime. Existing sidecars are never deleted. A second cooperating process receives `ReadOnly`; killing the owner releases the OS lock even though the sidecar remains.
- UUID-named same-directory stages use `create_new`, with a write handle that excludes other writers/deleters. Existing target DACLs are copied and protected **before any payload bytes are written**. Owner/group mismatches, hard links, read-only/reparse/encrypted/compressed/sparse/other special attributes are refused. Ordinary new files inherit directory permissions.
- Raw input/output is capped at 128 MiB. Writes finish, flush with `sync_all`, and verify complete staged bytes before replacement. The expected target bytes and link/attribute eligibility are checked before staging and again immediately before replacement. Only owned stages are cleaned up on preparation failure. Post-crash orphan stages/backups remain for explicit inspection; no recovery product is implied.

Canonical checks and pinned ancestors do not provide a sandbox against a malicious process with the same user authority deliberately replacing the final target/stage entry. Cooperating ProcessCraft writers are excluded; a non-cooperating editor can race the final check and replacement. No filesystem compare-and-swap or hardware power-loss guarantee is claimed. Actively synchronized folders cannot currently be detected and are not qualified. No automatic AV/indexer retry policy was added; a real sharing violation returns the explicit safe outcome.

### Actual and injected verification

Initial `cargo test -p processcraft-io --test storage` was the intended **RED**: E0432, the six storage exports did not exist. The manifest/workspace setup succeeded; this was missing behavior, not a tooling failure. The first implemented four required cases then passed.

| Check | Actual observation |
|---|---|
| `second_writer_is_read_only` | A separate copy of the Rust integration-test executable holds the real OS sidecar lock; parent is read-only; after `Child::kill` and wait, a writer opens normally |
| `actual_sharing_violation_keeps_original` | A real target handle omitting `FILE_SHARE_DELETE` produces Win32 error **32** at replacement; outcome is `NotCommitted`; original bytes intact |
| `actual_access_denied_during_stage_keeps_original` | `icacls` adds an actual current-user directory deny for `WD`/add-file after the writer opens; exclusive stage creation returns Win32 **5**; original and directory contents intact; deny removed before assertions/cleanup |
| `killed_writer_leaves_complete_old_or_new_bytes` | Actual child process termination at pre-replace and post-replace barriers; target is exactly 1 MiB old bytes before and exactly 1 MiB new bytes after; lock is reacquirable |
| `dacl_stream_and_creation_time_survive_replacement` | A protected current-user-only DACL is copied to the empty stage before payload; access rules match and stage ACL is protected. Final complete SDDL, an actual NTFS `:review` stream and creation time match the old file |
| Aliases, junction, directory pinning | Case and the Windows-supplied short path resolve the same sidecar; a trailing-dot/ADS grant is refused; a real junction ancestor is refused; renaming a granted directory produces sharing error 32 until grant release |
| New output and refusals | New target remains absent through all precommit hook points and appears complete after commit. External changes, actual hard links and actual read-only attributes are refused without target replacement |
| `failed_stage_keeps_original` | **Injected** errors at stage-created, written, flushed and before-replace points leave original bytes and remove owned stages |
| `injected_partial_stage_is_rejected` | **Injected** truncation of stage before validation is detected; original retained |
| Disk-full branch | **Injected** Win32 error **112** at written boundary yields `NotCommitted`; no physical disk was filled |
| Late replacement ambiguity unit test | **Injected** replacement operation moves original to backup then returns **1176**; target absent, old backup/new stage retained, result `Indeterminate`, blind retry refused. The host did **not** physically reproduce 1176/1177 |
| Postcommit warning | **Injected** error after successful replacement remains `Committed { warning: Some(...) }`; replacement bytes stay present |

The initial metadata test harness failed because this host's Windows PowerShell could not autoload `Microsoft.PowerShell.Security` for `Get-Acl`. The test now uses .NET access-control APIs without that module. A subsequent overly strict equality check exposed the difference between DACL auto-inheritance control flags on the protected stage (`D:P`) and original (`D:PAI`); it now checks exact access rules plus protection before writing and the full final original SDDL after replacement. These harness failures are not counted as passing results. The directory-pinning regression exposed a real adapter defect: a desired-access-zero directory handle did not block renames despite omitting share-delete. The adapter now requests `FILE_LIST_DIRECTORY`; the actual rename produces error 32 until handle release. This was fixed before acceptance, not waived.

Final focused commands: `cargo test -p processcraft-io --test storage` (17 integration cases including the child helper); `cargo test -p processcraft-io --locked --offline` (the original 16 cases plus one late-error unit case); `cargo clippy -p processcraft-io --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `cargo check --workspace --locked --offline`. Each completed successfully after fixes. The final review advisory added explicit refusal of all six superscript device names (`COM¹/²/³`, `LPT¹/²/³`), including extension forms, with a no-writes regression; focused storage tests, formatting and package Clippy passed after that change. [Microsoft's Windows naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) identify these names as reserved. No Git repository is present; no commit was created.

### Remaining qualification

The tests establish the fixture boundary for protected/custom DACLs, ADS, creation time, ordinary owner/group identity, and refusal of the listed unsupported attributes/links. SACL audit policy, mandatory integrity labels, extended attributes, arbitrary owners/groups, object IDs and every NTFS metadata class have not all been physically exercised; **this is not a transparent arbitrary-metadata save API qualification**. Owner/group mismatch refusal is implemented but not separately reproduced with a differently owned fixture. An enabled product save flow must remain within the qualified boundary or add detection/refusal/preservation and evidence for those cases. Recovery, complete package finalization/reopen, cryptographic disk digests, generation/dirty binding and reconciliation UX belong to later tasks. G1–G4/M0 completion cannot be inferred from the G2 tests alone.

## Task 03: bounded BPMN source preservation

**G3: PASS for the qualified read-only source-preservation subset.** All accepted imports remain `PreservedSource`; original XML bytes are the authority and semantic edits are unavailable, including for the minimal fixture. This enables no editable BPMN profile, domain graph, renderer or GUI import flow. It is not full BPMN XSD validation, a complete document engine, or a broad interchange compatibility claim.

### Boundary and dependencies

Pinned `quick-xml = 0.42.0`, default features disabled, declared MIT license; its normal dependency is the already resolved `memchr 2.8.3` (MIT OR Unlicense). Inspected the fetched package's `NsReader`, namespace resolver, attribute normalization and event APIs before implementing the adapter. No XML networking, schema resolver, encoding conversion, JavaScript runtime or external BPMN runtime is added to the application. The bounded API accepts UTF-8 XML 1.0 bytes; other encodings/versions are refused.

`import_bpmn` prepares temporary state and returns an immutable source document only after complete parsing and scoped checks succeed. It opens no files or URLs. The report states the read-only mode, unavailable semantic edits and validation/visual limitations, counts standard IDs/diagrams, and records resolved QName values and foreign element names by namespace URI plus local name. Namespace prefix rebinding never becomes identity. Standard references are checked only on explicitly recognized BPMN/DI elements and attributes or reference-text elements. Vendor `id`/`sourceRef` attributes are opaque, including when attached to standard elements. External-namespace references are refused instead of being fetched or accidentally matched by local name.

Limits enforced before returning a document: 32 MiB raw XML; 128 nesting levels; 10,000 total XML elements (conservatively includes extensions and DI); 16 KiB labels and 1 MiB individual documentation across text/CDATA/entity events. Additional M0 bounds are 16 KiB names/reference values/namespace URIs, a 32 MiB expanded-metadata accounting budget, and finite coordinates/sizes with absolute magnitude at most 1,000,000,000 diagram units; sizes must be non-negative. Known bounds and waypoints require their numeric fields. Invalid/duplicate standard IDs, unresolved known references, malformed namespaces/XML, unknown entities and every DTD are rejected. Numeric/predefined entity handling never resolves arbitrary entities. Errors identify the failing category and advise retaining the current document and inspecting corrected/smaller source. Malformed sources are rejected rather than retained as imported documents in this spike.

`export_source` returns the exact original bytes, including encoding declaration, whitespace, prefix spelling, CDATA, extensions and line endings. It cannot mutate a semantic model or write a destination. The test-only `preserve_bpmn` example imports each valid fixture, obtains bytes through this API, commits them through Task 02's explicit `FileGrant`/`WriterSession` boundary, and asserts readback byte equality.

### Fixtures and automated verification

Seven synthetic, locally authored fixtures are recorded in `tests/fixtures/manifest.json` with SHA-256, provenance, CC0-1.0 license, expected mode/diagram counts, semantic/DI checks, retention and permitted-edit expectations. They include minimal, extensions, multiple diagrams, unsupported timer event, namespace rebinding, malformed XML and external-DTD rejection. No vendor export, OMG MIWG model or other external fixture is claimed. Rendering is explicitly unqualified for every fixture.

Initial `cargo test -p processcraft-io --test bpmn` was the intended **RED** (E0432: missing `bpmn` module). Before the review follow-up, the suite passed ten integration tests, including all four required scenarios: `preserved_source_is_byte_identical`, `qnames_keep_namespace_identity`, `unsupported_content_never_becomes_editable`, `xml_limits_stop_before_commit`. Additional cases cover known-reference namespace identity and reference text, invalid IDs/geometry, vendor exclusions, malformed namespace/entity input, aggregate string limits, exact resource boundaries and multibyte UTF-8 byte counts. The follow-up increased the final BPMN count to eleven, as recorded below and in the aggregate. Failed imports leave an already imported source document unchanged; there is no domain/session transaction in this task.

At the initial Task 03 handoff these checks passed: `cargo test -p processcraft-io --test bpmn`; `cargo test -p processcraft-io --locked --offline` (then 10 BPMN tests, 17 storage integration cases and one storage unit case); `cargo clippy -p processcraft-io --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `cargo check --workspace --locked --offline`. The subsequent review follow-up passed eleven BPMN tests. Initial Clippy reported three collapsible conditional style issues, which were corrected before the passing run.

### Independent reopen of actual exports

Generated actual preserved outputs with:

```powershell
cargo run -p processcraft-io --example preserve_bpmn -- E:/Documenti/GitHub/ProcessCraft/target/m0-task03/preserved
```

The root reviewer independently reopened these actual output files using **bpmn-moddle 10.3.1 on Node 24.11.1** and checked source/output SHA-256 identity independently of the Rust assertions. All five outputs reopened as `bpmn:Definitions` with **zero warnings**. The raw report is `target/m0-task03/independent-reopen.json`; the consumer harness is test-only and has no runtime application dependency.

| Actual output | Diagrams | Main-process flow elements | Byte identity |
|---|---:|---:|---|
| minimal.bpmn | 1 | 5 | SHA-256 equal, 1,447 bytes |
| extensions.bpmn | 1 | 5 | SHA-256 equal, 1,648 bytes |
| multiple-diagrams.bpmn | 2 | 5 | SHA-256 equal, 1,695 bytes |
| unsupported-event.bpmn | 1 | 5 | SHA-256 equal, 1,663 bytes |
| namespace-rebinding.bpmn | 0 | 0 | SHA-256 equal, 404 bytes |

This is one independent consumer/version combination and five synthetic source-preservation examples. It provides no visual qualification or proof that the consumer understands vendor extension semantics. Architecture section 20's two-consumer requirement for public compatibility claims remains unmet; no such claim is made.

The first output harness run safely refused `NotContentIndexed` inherited from the ignored `target` directory. This ordinary Windows attribute is outside the current Task 02 storage whitelist. Only the newly owned preservation-output directory and its initial empty lock file had that flag cleared; rerunning then committed all five outputs cleanly. The storage implementation was not changed or bypassed. This is an explicit M0 refusal limitation, not transparent production support for arbitrary ordinary-file metadata. No Git repository is present; no commit was created. G4 and aggregate M0 results are recorded separately in their sections.

### Task 03 review follow-up: namespace identity

Both independent reviews approved Task 03 and raised two QName advisories, now closed. Added a targeted regression that first failed on an accepted whitespace-containing namespace name, then passed after the fix. M0 now refuses whitespace-containing namespace names, including whitespace introduced by character references, instead of allowing raw namespace-attribute whitespace to affect identity. Normalized/decoded namespace declarations are also checked for reserved XML/XMLNS binding rules, preventing escaped spellings from bypassing raw resolver checks. Ordinary escaped vendor namespace URIs remain accepted and resolve to the decoded URI. This conservative namespace-name restriction is stated in the capability report; it is not a general URI/XSD validator.

The rebinding fixture now uses the distinct `v:After` QName after the nested rebinding. The regression verifies that its URI is the restored outer `urn:vendor:first`, rather than merely testing set membership for the earlier `v:Kind` values. Its recorded manifest SHA-256 is now `db4ab30d7efa2531846ba5ca2d630695bfb5d72e236f11ff3beabe052b52170f` (404 bytes). The example regenerated all five actual preserved outputs through the preservation API and storage boundary. Final focused verification passed: eleven BPMN tests, package all-target Clippy with warnings denied, and formatting. The root reviewer reran independent source/output SHA-256 checks and bpmn-moddle 10.3.1 reopening on all five final actual exports after this one-byte fixture change: PASS, exact hashes and zero warnings. The namespace-rebinding output is 404 bytes with the updated SHA-256 above; `target/m0-task03/independent-reopen.json` contains the refreshed final report. G3 remains PASS for the qualified read-only subset.

## Task 04: shared labels and CPU rendering

**G4: PASS for the M0 pinned-font shared-outline proof.** The root reviewer independently rendered and visually inspected the actual SVG and candidate PDF alongside the CPU PNG at 100% and 200%. This does not enable a BPMN diagram renderer, native canvas integration, or a production PDF export feature. The existing M0 native shell remains separate. G1–G4 now have scoped qualification evidence; no M1 work is implied.

### Text and scene decision

Adopted `cosmic-text = 0.19.0` with default features disabled and only `std` enabled. Its inspected manifest/source composes HarfRust 0.5.2, skrifa 0.40.0, Unicode bidi/segmentation/line breaking and fontdb; its MSRV is 1.89, below this workspace's pinned Rust 1.98.1. [Upstream API and implementation](https://docs.rs/cosmic-text/0.19.0/cosmic_text/) expose the positioned glyphs and source clusters directly. This existing Rust layout implementation avoids writing a custom shaper, bidi algorithm, or wrapping engine. It is the small approved architecture section 13 candidate-stack refinement, not a second scene/export text engine. The renderer directly uses the same pinned skrifa 0.40.0 for outlines. The desktop's internal skrifa 0.44.0 and HarfRust 0.12.0 remain separate existing dependencies; no GUI text types enter this package.

The font database is constructed from exactly three embedded fonts. The locale is fixed to `en-US`, generic sans is pinned to Noto Sans, and the explicit common fallback order is Noto Sans, Noto Sans Arabic, Noto Sans CJK SC. No system-font discovery, platform fallback catalog, fontconfig, or swash rasterizer is used. `Shaping::Advanced`, word-or-glyph wrapping, 1.6-em line height and disabled hinting are shared settings. Font sizes and glyph offsets remain logical floating-point coordinates; no sink snaps positions independently. Unknown requested families report a deterministic Noto Sans fallback. Actual selected fallback faces are reported per label. An uncovered cluster reports its code points and retains the visible font `.notdef` outline, never a silent successful-coverage claim.

`TextLayout` stores source labels, font identity, glyph IDs, source-line/UTF-8 cluster ranges, advances, bidi direction and positioned unhinted outlines. The small immutable scene stores background rectangles and placed labels. One read-only `scene.paths()` stream feeds tiny-skia 0.12.0 CPU rasterization, outlined SVG, and the evidence-only PDF adapter. SVG keeps escaped labels in metadata and has no external references or live text/font dependency. XML-illegal metadata controls become replacement characters. PDF uses the same paths with one coordinate-system flip; quadratic curves convert mathematically to cubic curves. It does not reshape text, rasterize a page image, or select PDF fonts.

`pdf-writer = 0.15.0` is a dev dependency used only by `examples/text_parity.rs`. Chosen over a full high-level font/PDF stack because M0 only needs to qualify identical vector paths. `sha2 = 0.10.9` and `serde_json = 1.0.154` are also evidence/test dependencies. The paths-only PDF intentionally has no font embedding, searchable text, Unicode extraction, tagged accessibility, pagination or production export/save contract. Those capabilities retain their later release gates. There is no new runtime PDF module, GUI dependency, CLI package, storage bypass in product code, or BPMN scene/domain model.

### Bundled assets and bounded API

Only unmodified OFL-1.1 fonts from pinned upstream revisions are bundled. `assets/fonts/ATTRIBUTION.md` retains the embedded copyright/designer/version/license records; both full upstream OFL license texts are beside it. `assets/fonts/provenance.json` and the `fonts` array added to the existing fixture manifest record exact source URLs, revisions and SHA-256. Existing BPMN entries remain intact. No installed Segoe font was copied.

| Font | Size | Upstream revision | SHA-256 |
|---|---:|---|---|
| NotoSans-Regular.ttf | 569,208 bytes | noto-fonts `ffebf8c1ee449e544955a7e813c54f9b73848eac` | `b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5` |
| NotoSansArabic-Regular.ttf | 240,456 bytes | same noto-fonts revision | `ceea25b464a656dc3b26849bab9356740401af62aedf1bfa8b7f0d9b75925b1b` |
| NotoSansCJKsc-Regular.otf | 16,437,364 bytes | noto-cjk `f8d157532fbfaeda587e826d4cd5b21a49186f7c` | `2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b` |

The intentionally small catalog qualifies the six synthetic CC0 samples in `tests/fixtures/fonts/samples.json`: Latin accents/ligatures, Arabic mixed RTL/Latin/digits, CJK, combining marks, a four-line wrapped label and deliberately unavailable U+10FFFF. Noto Sans has explicit Arabic/CJK coverage gaps, so these samples exercise real fallback. Full Unicode coverage and regional CJK typographic preferences are not claimed; this catalog consistently uses the Simplified Chinese face. The full CJK font adds approximately 16.4 MB to the executable asset payload; no premature font subsetting or font-management framework was added.

Public input boundaries reject labels over 16 KiB UTF-8, family names over 128 bytes, non-finite or out-of-range font sizes (4–256) and wrap widths (4–16,384). A layout is capped at 32,768 glyphs, one million outline segments and one million logical pixels of height. A scene has at most 256 labels, 32,768 total glyphs, one million label-outline segments and 256 background rectangles. Placement/rectangle coordinates must be finite within +/-1,000,000; rectangles require positive dimensions. Artboards are 1–16,384 logical pixels per axis. Raster scale is 0.125–8, each raster axis is at most 16,384 pixels and RGBA allocation is capped at 64 MiB before allocating. Failed insertion leaves the scene unchanged. These are conservative M0 bounds, not large-diagram performance qualification.

### Automated proof and artifacts

Initial `cargo test -p processcraft-render --test text_parity` was the intended **RED**, E0432 for missing `DiagramScene`, `TextEngine`, `TextRequest`, after dependencies compiled successfully. Implemented `same_layout_drives_all_outputs` and `missing_glyph_reports_fallback` then passed. The final seven integration cases additionally check combining/precomposed glyph equivalence and clusters, bidi levels, wrapping, explicit missing-family fallback, input/raster/scene limits with no mutation, pinned font/corpus hashes and SVG metadata escaping. The example's separate test checks that PDF consumes exactly the shared path stream and contains vector paths without image/font objects.

Reproduce the actual outputs with the exact planned command:

```powershell
cargo run -p processcraft-render --example text_parity -- --out target/m0-text
```

It writes `text-parity-100.png` (960 x 860), `text-parity-200.png` (1920 x 1720), `text-parity.svg`, `text-parity.pdf` (one 960 x 860 point page) and `layout-evidence.json`. The evidence includes font/glyph/cluster/position records, diagnostics, path count and a SHA-256 over the one logical path stream. Console diagnostics name Arabic/CJK fallback and U+10FFFF; the wrapping sample has exactly four lines. The example writes only owned test artifacts using the standard library; it is not a product save flow.

Passing focused checks: `cargo test -p processcraft-render --all-targets` (seven integration tests plus one candidate-PDF example test); `cargo clippy -p processcraft-render --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `cargo check --workspace --locked --offline`. Milestone integration also passed `cargo test --workspace --all-targets --locked --offline`: **40 tests**, comprising three core ID tests, one storage unit test, eleven BPMN integration tests, seventeen storage integration tests, seven render integration tests and one candidate-PDF example test. No Git repository is present; no commit was created.

### Independent vector rendering and visual inspection

The root reviewer rendered the actual outlined SVG with the installed ArtCraft `VectorCraft/vectorcraft-cli.exe`, `--safe-mode run --in ... --export ... --scale 1` and `--scale 2`, with `VECTORCRAFT_NO_PREFS=1`. Both runs reported **zero import/export warnings**. That executable has no FileVersion metadata; its SHA-256 is `6D8EB6A8B9A9FCC28AF24617AD385A23F39843F160852C69D9A3ADDFDDB08A61`, identifying the actual consumer binary. Poppler **26.07.0** `pdfinfo` independently reported one 960 x 860 point PDF 1.7 page, no JavaScript and no encryption. `pdftoppm -r 72` and `-r 144` rendered the actual PDF at matching dimensions.

The reviewer visually inspected all six native/independent images at 100% and 200%: accents, Arabic mixed RTL/digits, CJK, combining marks, four-line wrapping and the intentional missing-glyph box agree without placement drift or clipping. Independent outputs are `target/m0-text/independent-{svg,pdf}-{100,200}.png`; numeric comparisons are in `independent-comparison.json`.

| Consumer / scale | Matching dimensions | Mean absolute RGB difference (out of 255) | Dark-ink bounds |
|---|---|---:|---|
| SVG 100% | 960 x 860 | 0.0901 | identical |
| PDF 100% | 960 x 860 | 0.7594 | identical |
| SVG 200% | 1920 x 1720 | 0.2438 | at most one pixel at right antialias edge |
| PDF 200% | 1920 x 1720 | 0.3641 | at most one pixel at right antialias edge |

Different rasterizer antialiasing is expected; no byte-identical image claim is made. This proves the bounded font/layout/vector path, with one independent consumer per vector format. It does not qualify public interchange compatibility, all scripts, PDF text semantics, or native monitor DPI interaction. The first direct headless Chrome attempt was automatically blocked and MCP browser connection timed out; neither is counted as successful evidence. The installed independent VectorCraft consumer supplied the SVG qualification without adding an application browser dependency.

### Resolved render dependency inventory

`target/m0-text/render-target-dependencies.txt` and `render-features.txt` record the actual Windows target graph/features. `render-dependencies.json` maps that graph to package-declared licenses from Cargo metadata. Raw workspace metadata alone includes inactive and other-platform dependencies, so it is not used as proof of active render features. Runtime has cosmic-text `std,sys-locale`, direct skrifa `std`, tiny-skia `std,simd,png-format` and unicode-script without optional features. Cosmic's `swash` and `fontconfig` features are disabled; swash entries present in the workspace lockfile are not an active render edge. The sys-locale crate is a required cosmic `std` edge, but this adapter passes the fixed locale rather than asking it for the host locale. `fontdb`'s fs/memmap features are transitive, while this adapter loads embedded memory bytes only.

The following table records the complete normal/build closure for this render package on the tested Windows target. Licenses are dependency package declarations; font licenses are separately recorded above. Build macros are included. Dev-only direct packages: pdf-writer 0.15.0 (MIT OR Apache-2.0), serde_json 1.0.154 (MIT OR Apache-2.0), sha2 0.10.9 (MIT OR Apache-2.0).

| Package | Version | Enabled features | Declared license |
|---|---|---|---|
| adler2 | 2.0.1 |  | 0BSD OR MIT OR Apache-2.0 |
| arrayref | 0.3.9 |  | BSD-2-Clause |
| arrayvec | 0.7.8 |  | MIT OR Apache-2.0 |
| bitflags | 2.13.2 |  | MIT OR Apache-2.0 |
| bytemuck | 1.25.2 | aarch64_simd,bytemuck_derive,derive,extern_crate_alloc,min_const_generics | Zlib OR Apache-2.0 OR MIT |
| bytemuck_derive | 1.12.1 |  | Zlib OR Apache-2.0 OR MIT |
| cfg-if | 1.0.5 |  | MIT OR Apache-2.0 |
| core_maths | 0.1.1 |  | MIT |
| cosmic-text | 0.19.0 | std,sys-locale | MIT OR Apache-2.0 |
| crc32fast | 1.5.5 | default,std | MIT OR Apache-2.0 |
| fdeflate | 0.3.7 |  | MIT OR Apache-2.0 |
| flate2 | 1.1.11 | any_impl,default,miniz_oxide,runtime_detection,rust_backend | MIT OR Apache-2.0 |
| font-types | 0.11.3 | bytemuck,std | MIT OR Apache-2.0 |
| fontdb | 0.23.0 | fs,memmap,memmap2,std | MIT |
| harfrust | 0.5.2 | std | MIT |
| libm | 0.2.16 | arch,default | MIT |
| linebender_resource_handle | 0.1.1 | std | Apache-2.0 OR MIT |
| log | 0.4.35 |  | MIT OR Apache-2.0 |
| memmap2 | 0.9.12 |  | MIT OR Apache-2.0 |
| miniz_oxide | 0.8.9 | default,simd,simd-adler32,with-alloc | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | default,simd,simd-adler32,with-alloc | MIT OR Zlib OR Apache-2.0 |
| png | 0.18.1 |  | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | default,proc-macro | MIT OR Apache-2.0 |
| quote | 1.0.47 | default,proc-macro | MIT OR Apache-2.0 |
| rangemap | 1.8.0 |  | MIT/Apache-2.0 |
| read-fonts | 0.37.0 | libm,std | MIT OR Apache-2.0 |
| rustc-hash | 2.1.3 |  | Apache-2.0 OR MIT |
| self_cell | 1.3.0 |  | Apache-2.0 OR GPL-2.0-only |
| simd-adler32 | 0.3.10 | const-generics,default,std | MIT |
| skrifa | 0.40.0 | std | MIT OR Apache-2.0 |
| slotmap | 1.1.1 |  | Zlib |
| smallvec | 1.16.5 |  | MIT OR Apache-2.0 |
| smol_str | 0.3.7 |  | MIT OR Apache-2.0 |
| strict-num | 0.1.1 |  | MIT |
| syn | 3.0.10 | clone-impls,default,derive,parsing,printing,proc-macro | MIT OR Apache-2.0 |
| sys-locale | 0.3.2 |  | MIT OR Apache-2.0 |
| tiny-skia | 0.12.0 | default,png-format,simd,std | BSD-3-Clause |
| tiny-skia-path | 0.12.0 | std | BSD-3-Clause |
| tinyvec | 1.13.3 | alloc,default | Zlib OR Apache-2.0 OR MIT |
| ttf-parser | 0.25.1 | apple-layout,core_maths,glyph-names,no-std-float,opentype-layout,std,variable-fonts | MIT OR Apache-2.0 |
| unicode-bidi | 0.3.18 | hardcoded-data,std | MIT OR Apache-2.0 |
| unicode-ident | 1.0.27 |  | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-linebreak | 0.1.5 |  | Apache-2.0 |
| unicode-script | 0.5.9 |  | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 |  | MIT OR Apache-2.0 |
| version_check | 0.9.5 |  | MIT/Apache-2.0 |

Task 04 independent specification-compliance and code-quality reviews both approved. The code-quality advisory was documentation-only: Glyph.cluster now explicitly states that its UTF-8 byte offsets are local to the original source_line, not global layout-text offsets. No behavior changed. The root reviewer also passed workspace all-target Clippy with --locked --offline and warnings denied, and independently verified all eleven fixture/font/corpus SHA-256 entries. The raster ceiling remains 64 MiB RGBA (16,777,216 pixels); no larger product export ceiling is enabled by this M0 spike.
