# Platify

[![Crates.io](https://img.shields.io/crates/v/platify.svg)](https://crates.io/crates/platify)
[![Documentation](https://docs.rs/platify/badge.svg)](https://docs.rs/platify)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![GitHub Repo](https://img.shields.io/badge/GitHub-Repository-black?logo=github)](https://github.com/mematthias/platify-rs)
[![Test Status](https://github.com/mematthias/platify-rs/actions/workflows/rust-check.yml/badge.svg)](https://github.com/mematthias/platify-rs/actions/workflows/rust-check.yml)

**Platify** streamlines cross-platform Rust development by removing the boilerplate associated with `#[cfg(...)]` attributes.

Instead of cluttering your code with repetitive checks and manual dispatch logic, Platify allows you to define platform-specific behavior using a clean, declarative attribute syntax.

## Features

- **`#[sys_function]`**: Automatically dispatches method calls to platform-specific implementations (e.g., `fn run()` -> `fn run_impl()`).
- **`#[sys_trait_function]`**: Applies platform configuration to methods within a trait definition.
- **`#[sys_struct]`**: Generates platform-specific type aliases (e.g., `MyStruct` -> `MyStructLinux`) and **verifies trait implementations** at compile time.
- **`#[platform_mod]`**: Declares modules backed by OS-specific files (e.g., `linux.rs`, `windows.rs`) with strict visibility control.
- **Smart Logic**: Supports explicit `include` and `exclude` lists.
- **Group Keywords**: Use helpers like `posix` (Linux + macOS) or `all`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
platify = "0.2.0"
```

Or run:

```bash
cargo add platify
```

## Usage

### 1. Platform-Dependent Functions (`#[sys_function]`)

This macro generates a default method that delegates the call to a suffixed implementation (e.g., `_impl`). It automatically applies the correct `#[cfg]` guards based on your configuration.

```rust
use platify::sys_function;

struct SystemManager;

impl SystemManager {
    // 1. Available on ALL supported platforms (default).
    //    Delegates to `reboot_impl`.
    #[sys_function]
    pub fn reboot(&self) -> Result<(), String>;

    // 2. ONLY available on Linux.
    #[sys_function(include(linux))]
    pub fn update_kernel(&self);

    // 3. Available on Linux and macOS, but NOT on Windows.
    #[sys_function(exclude(windows))]
    pub fn posix_magic(&self);
}

// Implementation details (usually handled in separate files or cfg blocks)
impl SystemManager {
    fn reboot_impl(&self) -> Result<(), String> {
        println!("Rebooting...");
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn update_kernel_impl(&self) {
        println!("Updating Linux kernel...");
    }

    #[cfg(unix)]
    fn posix_magic_impl(&self) {
        println!("Doing POSIX magic...");
    }
}
```

### 2. Platform-Specific Struct Aliases & Checks (`#[sys_struct]`)

This macro does two things:
1.  Creates type aliases for platform-specific builds (e.g., `HandleWindows`).
2.  **Verifies** that the struct implements specific traits (like `Send` or `Sync`) at compile time. This is crucial for FFI wrappers where thread-safety is easily accidentally broken.

```rust
use platify::sys_struct;

// 1. Generates `HandleWindows` alias on Windows.
// 2. Asserts at compile time that `Handle` implements `Send` and `Sync`.
//    This works even with generics!
#[sys_struct(include(windows), traits(Send, Sync))]
pub struct Handle<T> {
    handle: u64,
    _marker: std::marker::PhantomData<T>,
}

// Generated code roughly looks like:
//
// #[cfg(target_os = "windows")]
// pub type HandleWindows<T> = Handle<T>;
//
// #[cfg(target_os = "windows")]
// const _: () = { ... assert T: Send + Sync ... };
```

### 3. Trait Definitions (`#[sys_trait_function]`)

Allows you to define methods in a trait that are only available on specific platforms.

```rust
use platify::sys_trait_function;

trait DesktopEnv {
    // This method will only exist in the trait definition on Linux
    #[sys_trait_function(include(linux))]
    fn get_wm_name(&self) -> String;
}
```

### 4. Platform-Dependent Modules (`#[platform_mod]`)

Maps a logical module to a platform-specific file (e.g., `mod driver` maps to `linux.rs` or `windows.rs`).

**Visibility Logic:**
This macro separates internal convenience from external access:
1.  **External:** The specific module (e.g., `linux`) inherits the visibility you declared (`pub`), so users must import `crate::linux::Device`.
2.  **Internal:** The logical alias (`driver`) is generated as **private**, ensuring your internal code remains generic while forcing external users to be explicit about platform dependencies.

```rust
// Expects src/linux.rs and src/windows.rs to exist.
#[platform_mod(include(linux, windows))]
pub mod driver;

// --- Internal Usage ---
// Inside this file, we use the generic private alias.
fn init() {
    let _ = driver::Device::new();
}
```

**Consumer Usage (External Crate):**

```rust
// Error: 'driver' is private.
// use my_crate::driver::Device;

// Correct: The platform module is public.
#[cfg(target_os = "linux")]
use my_crate::linux::Device;
```

## Configuration Logic

You can control which platforms are targeted using `include(...)` and `exclude(...)`.

| Keyword | Description |
| :--- | :--- |
| `linux` | Target Linux (`target_os = "linux"`) |
| `windows` | Target Windows (`target_os = "windows"`) |
| `macos` | Target macOS (`target_os = "macos"`) |
| `posix` | Expands to `linux` and `macos` |
| `all` | Expands to `linux`, `macos`, and `windows` |

### How it is calculated

1.  **Start**: If `include` is present, start with that set. If omitted, start with `all`.
2.  **Filter**: Remove any platforms specified in `exclude`.
3.  **Result**: The macro generates `#[cfg(any(target_os = "..."))]` for the remaining platforms.

#### Examples

*   `include(linux)` → Only Linux.
*   `exclude(windows)` → Linux + macOS.
*   `include(posix), exclude(macos)` → Only Linux.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
