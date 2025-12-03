#![deny(deprecated)]

use platify::{sys_function, sys_struct};

// =========================================================================
// TEST 1: Basics & Dispatching
// Checks if the method call is correctly forwarded to the _impl method.
// =========================================================================

struct SimpleMath;

impl SimpleMath {
	// Default: include(all)
	#[sys_function]
	pub fn add(&self, a: i32, b: i32) -> i32;

	// The implementation to which the call is delegated
	fn add_impl(&self, a: i32, b: i32) -> i32 {
		a + b
	}
}

#[test]
fn test_basic_dispatch() {
	let math = SimpleMath;
	assert_eq!(math.add(10, 20), 30);
}

// =========================================================================
// TEST 2: Argument Forwarding & Mutability
// Checks if 'mut' is correctly stripped from arguments during the forwarding call.
// =========================================================================

struct StateProcessor;

impl StateProcessor {
	// Here we test:
	// 1. Arguments without explicit names in the pattern (handled by default).
	// 2. `mut` arguments (The macro must remove `mut` when forwarding).
	#[sys_function]
	#[expect(unused_mut, reason = "Test refers to this 'mut'")]
	fn process(&self, mut value: i32, factor: i32) -> i32;

	fn process_impl(&self, value: i32, factor: i32) -> i32 {
		value * factor
	}
}

#[test]
fn test_argument_cleaning() {
	let processor = StateProcessor;
	// If the macro generated `process_impl(mut value, factor)`,
	// this test would fail to compile due to syntax errors.
	assert_eq!(processor.process(10, 2), 20);
}

// =========================================================================
// TEST 3: Generics
// Checks if generics in function signatures are handled correctly.
// =========================================================================

struct GenericHandler;

impl GenericHandler {
	#[sys_function]
	fn wrap<T: Clone>(&self, item: T) -> (T, T);

	fn wrap_impl<T: Clone>(&self, item: T) -> (T, T) {
		(item.clone(), item)
	}
}

#[test]
fn test_generics() {
	let handler = GenericHandler;
	let result = handler.wrap("hello");
	assert_eq!(result, ("hello", "hello"));
}

// =========================================================================
// TEST 4: Return Types (Unit vs. Value)
// Checks if functions without a return value (Unit) correctly get a trailing semicolon.
// =========================================================================

struct SideEffect;
use std::cell::RefCell;

impl SideEffect {
	#[sys_function]
	fn trigger(&self, counter: &RefCell<i32>);

	fn trigger_impl(&self, counter: &RefCell<i32>) {
		*counter.borrow_mut() += 1;
	}
}

#[test]
fn test_unit_return() {
	let effect = SideEffect;
	let counter = RefCell::new(0);
	effect.trigger(&counter);
	assert_eq!(*counter.borrow(), 1);
}

// =========================================================================
// TEST 5: Async Support
// Checks if async/.await is correctly inserted into the generated code.
// =========================================================================

struct AsyncWorker;

impl AsyncWorker {
	#[sys_function]
	async fn fetch(&self) -> u8;

	async fn fetch_impl(&self) -> u8 {
		42
	}
}

#[tokio::test]
async fn test_async() {
	let worker = AsyncWorker;
	assert_eq!(worker.fetch().await, 42);
}

// =========================================================================
// TEST 6: Struct Aliases
// Checks if the aliases are created correctly for the CURRENT OS.
// =========================================================================

#[sys_struct(include(all))]
struct NativeHandle;

#[test]
fn test_struct_aliases() {
	let handle = NativeHandle;

	// This test is tricky because we can only check the alias
	// for the operating system the test is currently running on.

	#[cfg(target_os = "linux")]
	{
		// On Linux, this type alias must exist:
		let _alias: NativeHandleLinux = handle;
	}

	#[cfg(target_os = "macos")]
	{
		// On macOS, this type alias must exist:
		let _alias: NativeHandleMacOS = handle;
	}

	#[cfg(target_os = "windows")]
	{
		// On Windows, this type alias must exist:
		let _alias: NativeHandleWindows = handle;
	}
}

// =========================================================================
// TEST 7: Complex Exclusion Logic
// Checks if the exclusion logic works (compile-time check via cfg).
// =========================================================================

struct OsSpecific;

impl OsSpecific {
	// Exists everywhere EXCEPT on Windows.
	#[sys_function(exclude(windows))]
	fn unix_only(&self) -> bool;

	#[allow(dead_code)]
	fn unix_only_impl(&self) -> bool {
		true
	}
}

#[test]
fn test_exclusion() {
	let _os = OsSpecific;

	#[cfg(not(windows))]
	{
		assert!(_os.unix_only());
	}

	// On Windows, the method `unix_only` does not exist.
	// Calling it would result in a compile error, which proves the macro works.
}
