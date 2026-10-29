use super::memory;
use crate::error::{Error, Result};
use crate::{alloc, arch, util};
use std::cell::UnsafeCell;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

/// Follow common x64 entry-point detours before disassembling a target.
///
/// UE4SS/PolyHook2 can already replace a function entry with an indirect
/// absolute jump (`FF 25 disp32`). Building a second trampoline from that
/// six-byte stub relocates the stub's RIP-relative pointer and can fail even
/// though the underlying function is hookable. Following the chain lets a new
/// detour compose with the existing one instead.
#[cfg(target_arch = "x86_64")]
unsafe fn follow_entry_jumps(mut target: *const ()) -> *const () {
  for _ in 0..8 {
    let address = target as usize;
    let opcode = std::ptr::read(address as *const u8);
    let next = match opcode {
      // E9 rel32: direct near jump.
      0xE9 => {
        let displacement = std::ptr::read_unaligned((address + 1) as *const i32) as isize;
        (address as isize + 5 + displacement) as *const ()
      }
      // EB rel8: direct short jump.
      0xEB => {
        let displacement = std::ptr::read((address + 1) as *const i8) as isize;
        (address as isize + 2 + displacement) as *const ()
      }
      // FF 25 disp32: RIP-relative indirect absolute jump.
      0xFF if std::ptr::read((address + 1) as *const u8) == 0x25 => {
        let displacement = std::ptr::read_unaligned((address + 2) as *const i32) as isize;
        let slot = (address as isize + 6 + displacement) as *const usize;
        std::ptr::read_unaligned(slot) as *const ()
      }
      _ => break,
    };

    if next.is_null() || next == target {
      break;
    }
    target = next;
  }
  target
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn follow_entry_jumps(target: *const ()) -> *const () {
  target
}

/// An architecture-independent implementation of a base detour.
///
/// This class is never instantiated by itself, it merely exposes an API
/// available through it's descendants.
pub struct Detour {
  #[allow(dead_code)]
  relay: Option<alloc::ExecutableMemory>,
  trampoline: alloc::ExecutableMemory,
  patcher: UnsafeCell<arch::Patcher>,
  enabled: AtomicBool,
}

impl Detour {
  pub unsafe fn new(target: *const (), detour: *const ()) -> Result<Self> {
    let target = follow_entry_jumps(target);
    if target == detour {
      Err(Error::SameAddress)?;
    }

    // Lock this so OS operations are not performed in parallell
    let mut pool = memory::POOL.lock().unwrap();

    if !util::is_executable_address(target)? || !util::is_executable_address(detour)? {
      Err(Error::NotExecutable)?;
    }

    // Create a trampoline generator for the target function
    let margin = arch::meta::prolog_margin(target);
    let trampoline = arch::Trampoline::new(target, margin)?;

    // A relay is used in case a normal branch cannot reach the destination
    let relay = if let Some(emitter) = arch::meta::relay_builder(target, detour)? {
      Some(memory::allocate_pic(&mut pool, &emitter, target)?)
    } else {
      None
    };

    // If a relay is supplied, use it instead of the detour address
    let detour = relay
      .as_ref()
      .map(|code| code.as_ptr() as *const ())
      .unwrap_or(detour);

    Ok(Detour {
      patcher: UnsafeCell::new(arch::Patcher::new(
        target,
        detour,
        trampoline.prolog_size(),
      )?),
      trampoline: memory::allocate_pic(&mut pool, trampoline.emitter(), target)?,
      enabled: AtomicBool::default(),
      relay,
    })
  }

  /// Enables the detour.
  pub unsafe fn enable(&self) -> Result<()> {
    self.toggle(true)
  }

  /// Disables the detour.
  pub unsafe fn disable(&self) -> Result<()> {
    self.toggle(false)
  }

  /// Returns whether the detour is enabled or not.
  pub fn is_enabled(&self) -> bool {
    self.enabled.load(Ordering::SeqCst)
  }

  /// Returns a reference to the generated trampoline.
  pub fn trampoline(&self) -> &() {
    unsafe {
      (self.trampoline.as_ptr() as *const ())
        .as_ref()
        .expect("trampoline should not be null")
    }
  }

  /// Enables or disables the detour.
  unsafe fn toggle(&self, enabled: bool) -> Result<()> {
    let _guard = memory::POOL.lock().unwrap();

    if self.enabled.load(Ordering::SeqCst) == enabled {
      return Ok(());
    }

    // Runtime code is by default only read-execute
    let _handle = {
      let area = (*self.patcher.get()).area();
      region::protect_with_handle(
        area.as_ptr(),
        area.len(),
        region::Protection::READ_WRITE_EXECUTE,
      )
    }?;

    // Copy either the detour or the original bytes of the function
    (*self.patcher.get()).toggle(enabled);
    self.enabled.store(enabled, Ordering::SeqCst);
    Ok(())
  }
}

impl Drop for Detour {
  /// Disables the detour, if enabled.
  fn drop(&mut self) {
    debug_assert!(unsafe { self.disable().is_ok() });
  }
}

impl fmt::Debug for Detour {
  /// Output whether the detour is enabled or not.
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(
      f,
      "Detour {{ enabled: {}, trampoline: {:?} }}",
      self.is_enabled(),
      self.trampoline()
    )
  }
}

unsafe impl Send for Detour {}
unsafe impl Sync for Detour {}
