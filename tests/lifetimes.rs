use retour::{lifetime_detour, Result};

#[repr(C)]
struct Object {
  value: i32,
  replacement: i32,
}

impl Object {
  #[inline(never)]
  unsafe extern "C" fn read(&self) -> &i32 {
    let object = unsafe { std::ptr::read_volatile(&self) };
    &object.value
  }

  #[inline(never)]
  unsafe extern "C" fn read_replacement(&self) -> &i32 {
    let object = unsafe { std::ptr::read_volatile(&self) };
    &object.replacement
  }

  #[inline(never)]
  unsafe extern "system" fn update(&mut self, amount: i32) -> &mut i32 {
    self.value += unsafe { std::ptr::read_volatile(&amount) };
    &mut self.value
  }

  #[inline(never)]
  unsafe extern "system" fn update_replacement(&mut self, amount: i32) -> &mut i32 {
    self.value -= unsafe { std::ptr::read_volatile(&amount) };
    &mut self.value
  }
}

lifetime_detour! {
  /// Shared native receiver and a result borrowed from it.
  #[derive(Debug)]
  struct Read: for<'a> unsafe extern "C" fn(object: &'a Object) -> &'a i32;
  struct Update: for<'a> unsafe extern "system" fn(object: &'a mut Object, amount: i32) -> &'a mut i32;
  struct Independent: for<'a, 'b> fn(first: &'a i32, second: &'b i32) -> (&'a i32, &'b i32);
  pub(crate) struct Nested: for<'a, 'b> fn(view: &'a &'b str) -> &'b str;
  struct NoResult: for<'a> extern fn(value: &'a mut i32,);
  #[cfg(any())]
  struct Excluded: for<'a> fn(value: &'a MissingType);
  #[cfg_attr(all(), cfg(any()))]
  struct ConditionallyExcluded: for<'a> fn(value: &'a MissingType);
}

#[test]
fn native_method_preserves_shared_receiver_and_borrowed_result() -> Result<()> {
  let hook = unsafe { Read::new(Object::read, Object::read_replacement)? };
  let object = Object {
    value: 7,
    replacement: 11,
  };
  assert!(!hook.is_enabled());
  unsafe {
    assert!(std::ptr::eq(hook.call(&object), &object.value));
    hook.enable()?;
    assert!(hook.is_enabled());
    assert!(std::ptr::eq(Object::read(&object), &object.replacement));
    assert!(std::ptr::eq(hook.call(&object), &object.value));
    hook.disable()?;
    assert!(!hook.is_enabled());
    assert!(std::ptr::eq(Object::read(&object), &object.value));
  }
  Ok(())
}

#[test]
fn native_method_preserves_mutable_receiver_and_borrowed_result() -> Result<()> {
  let hook = unsafe { Update::new(Object::update, Object::update_replacement)? };
  let mut object = Object {
    value: 10,
    replacement: 0,
  };
  unsafe {
    *hook.call(&mut object, 2) += 3;
    assert_eq!(object.value, 15);
    hook.enable()?;
    assert_eq!(*Object::update(&mut object, 4), 11);
    assert_eq!(*hook.call(&mut object, 5), 16);
    hook.disable()?;
    assert_eq!(*Object::update(&mut object, 1), 17);
  }
  Ok(())
}

#[test]
fn independent_input_lifetimes_are_not_collapsed() -> Result<()> {
  #[inline(never)]
  fn original<'a, 'b>(first: &'a i32, second: &'b i32) -> (&'a i32, &'b i32) {
    (unsafe { std::ptr::read_volatile(&first) }, second)
  }
  fn replacement<'a, 'b>(first: &'a i32, second: &'b i32) -> (&'a i32, &'b i32) {
    (first, second)
  }
  let hook = unsafe { Independent::new(original, replacement)? };
  let outer = 23;
  let retained;
  {
    let inner = 31;
    let (first, second) = hook.call(&outer, &inner);
    assert!(std::ptr::eq(second, &inner));
    retained = first;
  }
  assert!(std::ptr::eq(retained, &outer));
  Ok(())
}

#[test]
fn nested_borrow_can_outlive_the_outer_view_and_hook() -> Result<()> {
  #[inline(never)]
  fn original<'a, 'b>(view: &'a &'b str) -> &'b str {
    unsafe { std::ptr::read_volatile(view) }
  }
  fn replacement<'a, 'b>(view: &'a &'b str) -> &'b str {
    view
  }
  let owned = String::from("borrowed");
  let result;
  {
    let hook = unsafe { Nested::new(original, replacement)? };
    let view = owned.as_str();
    result = hook.call(&view);
  }
  assert_eq!(result, owned);
  Ok(())
}

#[test]
fn unit_return_and_implicit_c_abi_work() -> Result<()> {
  #[inline(never)]
  extern "C" fn original(value: &mut i32) {
    unsafe {
      std::ptr::write_volatile(value, *value + 1);
    }
  }
  extern "C" fn replacement(value: &mut i32) {
    *value += 2;
  }
  let hook = unsafe { NoResult::new(original, replacement)? };
  let mut value = 0;
  hook.call(&mut value);
  assert_eq!(value, 1);
  unsafe {
    hook.enable()?;
  }
  original(&mut value);
  assert_eq!(value, 3);
  hook.call(&mut value);
  assert_eq!(value, 4);
  unsafe {
    hook.disable()?;
  }
  Ok(())
}

#[test]
fn named_detours_remain_send_sync_and_static() {
  fn check<T: Send + Sync + 'static>() {}
  check::<Read>();
  check::<Update>();
}
