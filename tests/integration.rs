use platify::{sys_enum, sys_function, sys_struct, sys_trait_function};
use std::cell::RefCell;

// =========================================================================
// 1. TEST: #[sys_function] - Dispatching & Basics
// Checks if calls are forwarded to the `_impl` method and if return types
// (Value and Unit) are handled correctly.
// =========================================================================

struct SimpleMath;

impl SimpleMath {
    #[sys_function]
    pub fn add(&self, a: i32, b: i32) -> i32;

    fn add_impl(&self, a: i32, b: i32) -> i32 {
        a + b
    }

    #[sys_function]
    pub fn increment(&self, counter: &RefCell<i32>);

    fn increment_impl(&self, counter: &RefCell<i32>) {
        *counter.borrow_mut() += 1;
    }
}

#[test]
fn test_basic_dispatch() {
    let math = SimpleMath;
    assert_eq!(math.add(10, 20), 30);

    let counter = RefCell::new(5);
    math.increment(&counter);
    assert_eq!(*counter.borrow(), 6);
}

// =========================================================================
// 2. TEST: #[sys_function] - Generics, Lifetimes & Mutability
// Checks if the macro correctly handles generic parameters, lifetime
// elision/naming, and strips 'mut' from arguments in the call-site.
// =========================================================================

struct AdvancedHandler;

impl AdvancedHandler {
    #[sys_function]
    fn wrap_and_trim<'a, T: Clone>(&self, item: T, mut text: &'a str) -> (T, &'a str);

    fn wrap_and_trim_impl<'a, T: Clone>(&self, item: T, text: &'a str) -> (T, &'a str) {
        (item, text.trim())
    }
}

#[test]
fn test_advanced_signatures() {
    let handler = AdvancedHandler;
    let (item, text) = handler.wrap_and_trim(42_u64, "  hello  ");
    assert_eq!(item, 42);
    assert_eq!(text, "hello");
}

// =========================================================================
// 3. TEST: #[sys_function] - Async & Unsafe
// Verifies support for async/.await and unsafe block generation.
// =========================================================================

struct SystemLowLevel;

impl SystemLowLevel {
    #[sys_function]
    async fn get_code(&self) -> u8;

    async fn get_code_impl(&self) -> u8 {
        42
    }

    #[sys_function]
    unsafe fn raw_access(&self) -> bool;

    unsafe fn raw_access_impl(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn test_async_and_unsafe() {
    let sys = SystemLowLevel;
    assert_eq!(sys.get_code().await, 42);
    assert!(unsafe { sys.raw_access() });
}

// =========================================================================
// 4. TEST: #[sys_struct] & #[sys_enum] - Trait Assertions
// Verifies that 'traits(...)' generates a compile-time check for the
// specified bounds.
// =========================================================================

#[sys_struct(traits(Send, Sync))]
struct ThreadSafeData<T: Send + Sync> {
    inner: T,
}

#[sys_enum(traits(Clone, Copy))]
#[derive(Clone, Copy)]
enum SimpleStatus {
    Ok,
    Error,
}

#[test]
fn test_trait_assertions() {
    // These are primarily compile-time tests.
    let _data = ThreadSafeData { inner: 100 };
    let _status = SimpleStatus::Ok;
}

// =========================================================================
// 5. TEST: #[sys_trait_function]
// Verifies that the macro correctly applies #[cfg] to trait methods.
// =========================================================================

trait CrossPlatformDevice {
    #[sys_trait_function(include(linux, macos))]
    fn unix_id(&self) -> u32;

    #[sys_trait_function(include(windows))]
    fn win_id(&self) -> u32;
}

struct MyDevice;
impl CrossPlatformDevice for MyDevice {
    #[cfg(unix)]
    fn unix_id(&self) -> u32 {
        1
    }

    #[cfg(windows)]
    fn win_id(&self) -> u32 {
        2
    }
}

#[test]
fn test_trait_gating() {
    let dev = MyDevice;
    #[cfg(unix)]
    assert_eq!(dev.unix_id(), 1);
    #[cfg(windows)]
    assert_eq!(dev.win_id(), 2);
}

// =========================================================================
// 6. TEST: Visibility Propagation
// Ensures 'pub' and 'pub(crate)' are correctly applied to wrappers.
// =========================================================================

mod internal {
    use platify::sys_function;
    pub struct Hidden;

    impl Hidden {
        #[sys_function]
        pub(crate) fn internal_call(&self) -> bool;

        fn internal_call_impl(&self) -> bool {
            true
        }
    }
}

#[test]
fn test_visibility() {
    let h = internal::Hidden;
    // Should be accessible here as we are in the same crate
    assert!(h.internal_call());
}

// =========================================================================
// 7. TEST: Complex Platform Logic
// Verifies the exclude/include calculation.
// =========================================================================

struct LogicTest;

impl LogicTest {
    // Should exist on Linux because: (all) - windows - macos = linux
    #[sys_function(exclude(windows, macos))]
    fn only_linux(&self) -> bool;

    fn only_linux_impl(&self) -> bool {
        true
    }
}

#[test]
fn test_exclusion_logic() {
    let l = LogicTest;
    #[cfg(target_os = "linux")]
    assert!(l.only_linux());
}
