use platify::{sys_function, sys_struct};
use std::cell::RefCell;

// =========================================================================
// TEST: Basics & Dispatching
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
// TEST: Visibility Propagation
// Checks if visibility modifiers (like `pub`) are correctly preserved.
// We define the struct inside a module and try to access it from outside.
// =========================================================================

mod visibility_check {
    use platify::sys_function;

    pub struct PublicWorker;

    impl PublicWorker {
        // This method is marked 'pub'.
        // The macro must ensure the generated wrapper is also 'pub'.
        #[sys_function]
        pub fn do_public_work(&self) -> bool;

        // The implementation can remain private (default), as it is only
        // called by the wrapper (which is inside the same impl block).
        fn do_public_work_impl(&self) -> bool {
            true
        }
    }
}

#[test]
fn test_visibility_is_preserved() {
    let worker = visibility_check::PublicWorker;

    // Attempt to call the method from OUTSIDE the module.
    // This will only compile if the generated code preserved the `pub` keyword.
    assert!(worker.do_public_work());
}

// =========================================================================
// TEST: Argument Forwarding & Mutability
// Checks if 'mut' is correctly stripped from arguments during the forwarding call.
// =========================================================================

struct StateProcessor;

impl StateProcessor {
    // Here we test:
    // 1. Arguments without explicit names in the pattern (handled by default).
    // 2. `mut` arguments (The macro must remove `mut` when forwarding).
    #[sys_function]
    #[allow(unused_mut)]
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
// TEST: Generics
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
// TEST: Return Types (Unit vs. Value)
// Checks if functions without a return value (Unit) correctly get a trailing semicolon.
// =========================================================================

struct SideEffect;

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
// TEST: Async Support
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
// TEST: Complex Exclusion Logic
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

// =========================================================================
// TEST: Trait Assertions with Generics
// Verifies that 'traits(...)' works correctly with generic structs.
// This ensures the macro generates: fn _check<T: Clone>() { ... }
// instead of instantiating with dummy types.
// =========================================================================

#[sys_struct(traits(Send, Sync))]
struct GenericWrapper<T: Clone + Send + Sync> {
    data: T,
}

#[test]
fn test_generic_trait_assertion() {
    // This test is primarily a compile-time check.
    // If the macro generates invalid code (e.g., checking GenericWrapper<bool>),
    // compilation would fail for types that don't match the hardcoded dummy.
    let _wrapper = GenericWrapper { data: 42 };
}

#[sys_struct(traits(Send))]
struct UnsizedWrapper<T: ?Sized + Send> {
    data: Box<T>,
}

// =========================================================================
// TEST: Unsafe Support
// Checks if 'unsafe' functions are correctly wrapped in 'unsafe' blocks.
// =========================================================================

struct DangerZone;

impl DangerZone {
    #[sys_function]
    unsafe fn explode(&self) -> bool;

    unsafe fn explode_impl(&self) -> bool {
        true
    }
}

#[test]
fn test_unsafe() {
    let danger = DangerZone;
    // We must call it inside an unsafe block, confirming the signature kept 'unsafe'
    let result = unsafe { danger.explode() };
    assert!(result);
}

// =========================================================================
// TEST: Lifetimes & References
// Checks if arguments passed by reference are forwarded correctly.
// =========================================================================

struct StringParser;

impl StringParser {
    #[sys_function]
    fn parse<'a>(&self, input: &'a str) -> &'a str;

    fn parse_impl<'a>(&self, input: &'a str) -> &'a str {
        input.trim()
    }
}

#[test]
fn test_lifetimes() {
    let parser = StringParser;
    let data = "  hello  ";
    assert_eq!(parser.parse(data), "hello");
}
