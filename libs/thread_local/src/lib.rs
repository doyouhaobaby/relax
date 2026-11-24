//! Thread-local storage for Makepad and Relax.
//! Native Android code shares one pthread key across its thread-locals so
//! packed apps can fit within bionic's process-wide key limit.
//! Other targets and code compiled by Relax use std's thread-locals.

#[cfg(not(all(target_os = "android", not(relax))))]
pub use std::thread::{AccessError, LocalKey};

#[cfg(not(all(target_os = "android", not(relax))))]
/// Declares thread-local keys using std's syntax.
#[macro_export]
macro_rules! thread_local {
    ($($t:tt)*) => { ::std::thread_local! { $($t)* } };
}

#[cfg(any(all(target_os = "android", not(relax)), all(test, unix)))]
#[doc(hidden)]
pub mod table;

#[cfg(all(target_os = "android", not(relax)))]
pub use table::{AccessError, LocalKey};

#[cfg(all(target_os = "android", not(relax)))]
/// Declares thread-local keys that share the native Android table. A macro of its own, not a
/// `pub use` of the table's: `#[macro_use(thread_local)] extern crate` imports exported macros.
#[macro_export]
macro_rules! thread_local {
    ($($t:tt)*) => { $crate::__table_thread_local! { $($t)* } };
}

#[cfg(any(all(target_os = "android", not(relax)), all(test, unix)))]
#[doc(hidden)]
#[macro_export]
macro_rules! __table_thread_local {
    () => {};
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = const $init:block $(; $($rest:tt)*)?) => {
        $crate::__table_thread_local! {
            $(#[$attr])* $vis static $name: $t = { const $init } $(; $($rest)*)?
        }
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr $(; $($rest:tt)*)?) => {
        $(#[$attr])* $vis const $name: $crate::table::LocalKey<$t> = {
            static __MAKEPAD_SLOT: $crate::table::Slot<$t> = $crate::table::Slot::new();
            #[inline]
            fn __makepad_init() -> $t { $init }
            $crate::table::LocalKey::__new(&__MAKEPAD_SLOT, __makepad_init)
        };
        $($crate::__table_thread_local! { $($rest)* })?
    };
}
