# Platify

[![Crates.io](https://img.shields.io/crates/v/platify.svg)](https://crates.io/crates/platify)
[![Documentation](https://docs.rs/platify/badge.svg)](https://docs.rs/platify)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![GitHub Repo](https://img.shields.io/badge/GitHub-Repository-black?logo=github)](https://github.com/mematthias/platify-rs)
[![Test Status](https://github.com/mematthias/platify-rs/actions/workflows/rust-check.yml/badge.svg)](https://github.com/mematthias/platify-rs/actions/workflows/rust-check.yml)

**Platify** streamlines cross-platform Rust development by removing the boilerplate associated with `#[cfg(...)]` attributes.

Instead of cluttering your code with repetitive checks and manual dispatch logic, Platify allows you to define platform-specific behavior using a clean, declarative attribute syntax.

## Features

- **`#[sys_function]`**: Generates a wrapper that dispatches calls to platform-specific implementations (e.g., `fn run()` calls `Self::run_impl()`).
- **`#[sys_trait_function]`**: Applies platform configuration to methods within a trait definition.
- **`#[sys_struct]` / `#[sys_enum]`**: Applies platform gating to types and **verifies trait bounds** (e.g., `Send + Sync`) at compile time.
- **`#[platform_mod]`**: Declares modules backed by OS-specific files (e.g., `linux.rs`, `windows.rs`) with a unified internal alias and strict visibility control.
- **Smart Logic**: Supports explicit `include` and `exclude` lists with group keywords like `posix` and `all`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
platify = "0.4.0"
```

Or run:

```bash
cargo add platify
```

## Usage

### 1. Platform-Dependent Functions (`#[sys_function]`)

This macro generates a method body that delegates the call to an implementation suffixed with `_impl`. This keeps your public API clean while separating platform logic.

```rust
use platify::sys_function;

struct SystemManager;

impl SystemManager {
    // 1. Available on ALL supported platforms (default).
    //    Generates a body that calls `Self::reboot_impl(self)`.
    #[sys_function]
    pub fn reboot(&self) -> Result<(), String>;

    // 2. ONLY available on Linux.
    #[sys_function(include(linux))]
    pub fn update_kernel(&self);
}

// You implement the specific logic for each platform:
impl SystemManager {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn reboot_impl(&self) -> Result<(), String> {
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn update_kernel_impl(&self) {
        println!("Updating Linux kernel...");
    }
}
```

### 2. Platform-Gating & Trait Verification (`#[sys_struct]`)

Standard `#[cfg]` attributes can hide implementation errors until you compile for that specific platform. `#[sys_struct]` allows you to **assert** that a type implements certain traits (like `Send` or `Sync`) immediately.

```rust
use platify::sys_struct;

// 1. This struct only exists on Windows.
// 2. Asserts at compile time that `WinHandle` implements `Send` and `Sync`.
//    If it doesn't, you get a clear error during the build.
#[sys_struct(include(windows), traits(Send, Sync))]
pub struct WinHandle {
    handle: u64,
}
```

### 3. Trait Definitions (`#[sys_trait_function]`)

Use this to define methods in a trait that only exist when compiling for specific platforms.

```rust
use platify::sys_trait_function;

trait DesktopEnv {
    // This method only exists in the trait definition on Linux
    #[sys_trait_function(include(linux))]
    fn get_wm_name(&self) -> String;
}
```

### 4. Platform-Dependent Modules (`#[platform_mod]`)

This automates the common pattern of having a `linux.rs` and `windows.rs` and aliasing them to a common name for internal use.

**Visibility Logic:**
1.  **External:** The specific module (e.g., `mod linux;`) inherits the visibility you declared (`pub`). External users see `my_crate::linux`.
2.  **Internal:** The logical alias (`driver`) is generated as a **private alias** (`use linux as driver;`). Your code uses `driver::...` regardless of the OS.

```rust
// Expects src/linux.rs and src/windows.rs to exist.
#[platform_mod(include(linux, windows))]
pub mod driver;

// --- Internal Usage ---
fn init() {
    // Use the generic private alias 'driver' internally.
    let _ = driver::Device::new();
}
```

## Configuration Logic

You can control which platforms are targeted using `include(...)` and `exclude(...)`.

| Keyword | Target OS / Expansion |
| :--- | :--- |
| `linux` | `target_os = "linux"` |
| `windows` | `target_os = "windows"` |
| `macos` | `target_os = "macos"` |
| `posix` | `linux` + `macos` |
| `all` | `linux` + `macos` + `windows` |

### How it is calculated

1.  **Start**: Defaults to `all` if `include` is omitted.
2.  **Filter**: Removes any platforms specified in `exclude`.
3.  **Result**: Generates the appropriate `#[cfg(any(...))]` attribute.

**Examples:**
*   `include(linux)` → Only Linux.
*   `exclude(windows)` → Linux + macOS.
*   `include(posix), exclude(macos)` → Only Linux.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
