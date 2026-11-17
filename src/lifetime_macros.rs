/// Define named, typed detours with per-call borrow lifetimes.
///
/// Unlike [`GenericDetour`](crate::GenericDetour), these declarations preserve
/// a higher-ranked function pointer instead of decomposing it into fixed
/// argument and output types. Target and replacement must both satisfy the
/// complete `for<...>` signature. Each generated `call` has the declared
/// lifetime parameters, including relationships between arguments and returned
/// borrows.
///
/// # Syntax
///
/// ```rust
/// retour::lifetime_detour! {
///   pub struct Read: for<'a> unsafe extern "C" fn(value: &'a i32) -> &'a i32;
///   struct Update: for<'a> fn(value: &'a mut i32, amount: i32);
/// }
/// ```
///
/// Argument names and at least one explicit lifetime are required. Visibility,
/// documentation and `cfg` attributes are supported. Function-pointer ABIs are
/// passed through to Rust; unsupported conventions are rejected by the
/// compiler. Unsafe signatures generate unsafe `call` methods; safe signatures
/// generate safe `call` methods. This macro does not require the
/// `static-detour` feature.
///
/// # Example
///
/// ```rust
/// use retour::{lifetime_detour, Result};
///
/// lifetime_detour! {
///   struct Read: for<'a> fn(value: &'a i32) -> &'a i32;
/// }
///
/// #[inline(never)]
/// fn original(value: &i32) -> &i32 {
///   unsafe { std::ptr::read_volatile(&value) }
/// }
/// fn replacement(value: &i32) -> &i32 { value }
///
/// # fn main() -> Result<()> {
/// let hook = unsafe { Read::new(original, replacement)? };
/// let local = 7;
/// assert_eq!(*hook.call(&local), 7);
/// # Ok(())
/// # }
/// ```
///
/// Borrowed results cannot outlive their inputs:
///
/// ```compile_fail
/// retour::lifetime_detour! {
///   struct Read: for<'a> fn(value: &'a i32) -> &'a i32;
/// }
/// fn escape(hook: &Read) -> &'static i32 {
///   let local = 7;
///   hook.call(&local)
/// }
/// ```
///
/// A replacement accepting only static borrows is not general enough:
///
/// ```compile_fail
/// retour::lifetime_detour! {
///   struct Read: for<'a> fn(value: &'a i32) -> &'a i32;
/// }
/// fn original(value: &i32) -> &i32 { value }
/// fn replacement(value: &'static i32) -> &'static i32 { value }
/// let _ = unsafe { Read::new(original, replacement) };
/// ```
///
/// A target accepting only static borrows is also rejected:
///
/// ```compile_fail
/// retour::lifetime_detour! {
///   struct Read: for<'a> fn(value: &'a i32) -> &'a i32;
/// }
/// fn target(value: &'static i32) -> &'static i32 { value }
/// fn replacement(value: &i32) -> &i32 { value }
/// let _ = unsafe { Read::new(target, replacement) };
/// ```
///
/// Replacements must preserve which input owns a returned borrow:
///
/// ```compile_fail
/// retour::lifetime_detour! {
///   struct First: for<'a, 'b> fn(first: &'a i32, second: &'b i32) -> &'a i32;
/// }
/// fn target<'a, 'b>(first: &'a i32, _: &'b i32) -> &'a i32 { first }
/// fn replacement<'a, 'b>(_: &'a i32, second: &'b i32) -> &'b i32 { second }
/// let _ = unsafe { First::new(target, replacement) };
/// ```
///
/// Unsafe function contracts remain unsafe at the call site:
///
/// ```compile_fail
/// retour::lifetime_detour! {
///   struct Read: for<'a> unsafe extern "C" fn(value: &'a i32) -> &'a i32;
/// }
/// fn call_without_unsafe(hook: &Read, value: &i32) { hook.call(value); }
/// ```
///
/// A returned mutable borrow prevents a second mutable borrow of its input:
///
/// ```compile_fail
/// retour::lifetime_detour! {
///   struct Update: for<'a> fn(value: &'a mut i32) -> &'a mut i32;
/// }
/// fn alias(hook: &Update, value: &mut i32) {
///   let first = hook.call(value);
///   let second = hook.call(value);
///   *first += *second;
/// }
/// ```
///
/// # Safety
///
/// Construction and patching remain unsafe, as with `GenericDetour`. The native
/// target must uphold the declared ABI, validity, aliasing and lifetime rules
/// for every call. This macro does not validate native objects, extend their
/// lifetimes, support capturing closures, or synchronize patching with callers.
/// Keep the hook and target/replacement code alive until all calls have
/// finished.
#[macro_export]
macro_rules! lifetime_detour {
  // Preserve tokens so cfg attributes can also be applied to the impl.
  ($(#[$($attribute:tt)*])* $visibility:vis struct $name:ident:
      for<$($lifetime:lifetime),+ $(,)?> $($rest:tt)*) => {
    $crate::lifetime_detour!(@unsafe
      [$(#[$($attribute)*])*] [$visibility] [$name] [$($lifetime),+] | $($rest)*);
  };

  (@unsafe $attributes:tt $visibility:tt $name:tt $lifetimes:tt | unsafe $($rest:tt)*) => {
    $crate::lifetime_detour!(@abi $attributes $visibility $name $lifetimes [unsafe] | $($rest)*);
  };
  (@unsafe $attributes:tt $visibility:tt $name:tt $lifetimes:tt | $($rest:tt)*) => {
    $crate::lifetime_detour!(@abi $attributes $visibility $name $lifetimes [] | $($rest)*);
  };

  (@abi $attributes:tt $visibility:tt $name:tt $lifetimes:tt $safety:tt |
      extern $abi:literal fn $($rest:tt)*) => {
    $crate::lifetime_detour!(@signature $attributes $visibility $name $lifetimes $safety [extern $abi] | $($rest)*);
  };
  (@abi $attributes:tt $visibility:tt $name:tt $lifetimes:tt $safety:tt |
      extern fn $($rest:tt)*) => {
    $crate::lifetime_detour!(@signature $attributes $visibility $name $lifetimes $safety [extern "C"] | $($rest)*);
  };
  (@abi $attributes:tt $visibility:tt $name:tt $lifetimes:tt $safety:tt | fn $($rest:tt)*) => {
    $crate::lifetime_detour!(@signature $attributes $visibility $name $lifetimes $safety [] | $($rest)*);
  };

  (@signature [$($attributes:tt)*] [$visibility:vis] [$name:ident]
      [$($lifetime:lifetime),+] [$($safety:tt)*] [$($abi:tt)*] |
      ($($argument:ident: $argument_type:ty),* $(,)?) $(-> $output:ty)?; $($rest:tt)*) => {
    $($attributes)*
    $visibility struct $name {
      __retour_raw: $crate::RawDetour,
    }

    $crate::lifetime_detour!(@impl_attributes [] [
      impl $name {
        /// Create a disabled hook with the declared higher-ranked signature.
        ///
        /// # Safety
        /// Both functions must be valid hookable code with this exact ABI and
        /// lifetime contract. They must remain loaded until the hook is dropped
        /// and no calls may still be executing when it is dropped.
        pub unsafe fn new(
          target: for<$($lifetime),+> $($safety)* $($abi)* fn($($argument_type),*) $(-> $output)?,
          replacement: for<$($lifetime),+> $($safety)* $($abi)* fn($($argument_type),*) $(-> $output)?,
        ) -> $crate::Result<Self> {
          unsafe {
            $crate::RawDetour::new(target as *const (), replacement as *const ())
              .map(|raw| Self { __retour_raw: raw })
          }
        }

        /// Call the original through its trampoline, preserving borrow lifetimes.
        ///
        /// # Safety
        /// For unsafe signatures, the original function's requirements apply.
        #[inline]
        pub $($safety)* fn call<$($lifetime),+>(&self, $($argument: $argument_type),*) $(-> $output)? {
          let original: $($safety)* $($abi)* fn($($argument_type),*) $(-> $output)? = unsafe {
            ::std::mem::transmute(self.__retour_raw.trampoline())
          };
          #[allow(unused_unsafe)]
          unsafe { original($($argument),*) }
        }

        /// Enable redirection to the replacement.
        ///
        /// # Safety
        /// Synchronize patching with all callers; both functions must uphold
        /// the declared signature, including every borrow lifetime.
        pub unsafe fn enable(&self) -> $crate::Result<()> {
          unsafe { self.__retour_raw.enable() }
        }

        /// Restore the original target's entry.
        ///
        /// # Safety
        /// Synchronize patching with all callers, as for `RawDetour::disable`.
        pub unsafe fn disable(&self) -> $crate::Result<()> {
          unsafe { self.__retour_raw.disable() }
        }

        /// Return whether redirection is enabled.
        pub fn is_enabled(&self) -> bool { self.__retour_raw.is_enabled() }

        /// Return the untyped trampoline, borrowed from this hook.
        pub fn trampoline(&self) -> &() { self.__retour_raw.trampoline() }
      }
    ] | $($attributes)*);

    $crate::lifetime_detour!($($rest)*);
  };

  // Only conditional-compilation attributes belong on both items. Other
  // attributes (docs, derives, lint settings) belong on the declared struct.
  (@impl_attributes [$($cfg:tt)*] [$item:item] | #[cfg($($predicate:tt)*)] $($rest:tt)*) => {
    $crate::lifetime_detour!(@impl_attributes [$($cfg)* #[cfg($($predicate)*)]] [$item] | $($rest)*);
  };
  (@impl_attributes [$($cfg:tt)*] [$item:item] |
      #[cfg_attr($condition:meta, cfg($($predicate:tt)*))] $($rest:tt)*) => {
    $crate::lifetime_detour!(@impl_attributes
      [$($cfg)* #[cfg_attr($condition, cfg($($predicate)*))]] [$item] | $($rest)*);
  };
  (@impl_attributes $cfg:tt $item:tt | #[$attribute:meta] $($rest:tt)*) => {
    $crate::lifetime_detour!(@impl_attributes $cfg $item | $($rest)*);
  };
  (@impl_attributes [$($cfg:tt)*] [$item:item] |) => { $($cfg)* $item };
  () => {};
}
