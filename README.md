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
- **`#[sys_struct]`**: Generates platform-specific type aliases (e.g., `MyStruct` -> `MyStructLinux`).
- **Smart Logic**: Supports explicit `include` and `exclude` lists.
- **Group Keywords**: Use helpers like `posix` (Linux + macOS) or `all`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
platify = "0.1.1"
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

// Implementation details
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

### 2. Platform-Specific Struct Aliases (`#[sys_struct]`)

Useful when you need specific types for FFI or OS interactions but want to keep a unified naming convention in your platform-agnostic code.

```rust
use platify::sys_struct;

#[sys_struct(include(windows))]
pub struct Handle {
	handle: u64
}

// This generates:
//
// #[cfg(target_os = "windows")]
// pub type HandleWindows = Handle;
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
