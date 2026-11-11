//! The underlying disassembler should be opaque to the outside.
use crate::{
  error::{Error, Result},
  util,
};
use iced_x86::{
  Code, Decoder, DecoderError, DecoderOptions, Instruction, Mnemonic, OpKind, Register,
};
use std::collections::HashSet;

pub trait InstructionExt {
  /// Returns the instructions relative branch offset, if applicable.
  fn relative_branch_target(&self) -> Option<u64>;
  /// Returns the instructions RIP operand displacement if applicable.
  fn rip_operand_target(&self) -> Option<u64>;
  /// Returns true if this instruction any type of a loop.
  fn is_loop(&self) -> bool;
  /// Returns true if this instruction is an unconditional jump.
  fn is_unconditional_jump(&self) -> bool;
  /// Returns true if this instruction is a function call.
  fn is_call(&self) -> bool;
  /// Returns true if this instruction is a return.
  fn is_return(&self) -> bool;
}

impl InstructionExt for Instruction {
  /// Returns the instructions relative branch offset, if applicable.
  fn relative_branch_target(&self) -> Option<u64> {
    use OpKind::*;
    match self.op0_kind() {
      NearBranch16 | NearBranch32 | NearBranch64 => Some(self.near_branch_target()),
      _ => None,
    }
  }

  /// Returns the instructions RIP operand displacement if applicable.
  fn rip_operand_target(&self) -> Option<u64> {
    self
      .op_kinds()
      .find(|op| *op == OpKind::Memory && self.memory_base() == Register::RIP)
      .map(|_| self.memory_displacement64())
  }

  /// Returns true if this instruction any type of a loop.
  fn is_loop(&self) -> bool {
    use Mnemonic::*;
    matches!(self.mnemonic(), Loop | Loope | Loopne | Jecxz | Jcxz)
  }

  /// Returns true if this instruction is an unconditional jump.
  fn is_unconditional_jump(&self) -> bool {
    self.mnemonic() == Mnemonic::Jmp
  }

  /// Returns true if this instruction is a function call.
  fn is_call(&self) -> bool {
    self.mnemonic() == Mnemonic::Call
  }

  /// Returns true if this instruction is a return.
  fn is_return(&self) -> bool {
    self.mnemonic() == Mnemonic::Ret
  }
}

/// Other systems may apply detours or other binary patches at runtime, so we
/// need to follow their jumps to the end of the chain to determine where to
/// insert our detour.
pub unsafe fn follow_entry_jumps(mut target: *const ()) -> Result<*const ()> {
  let mut visited = HashSet::new();
  loop {
    if target.is_null() || !visited.insert(target) {
      return Err(Error::InvalidCode);
    }
    if !util::is_executable_address(target)? {
      return Err(Error::NotExecutable);
    }
    let instruction = decode_instruction(target.cast())?;
    if !instruction.is_unconditional_jump() {
      return Ok(target);
    }
    target = if let Some(destination) = instruction.relative_branch_target() {
      destination as *const ()
    } else {
      if !matches!(instruction.code(), Code::Jmp_rm32 | Code::Jmp_rm64)
        || instruction.op0_kind() != OpKind::Memory
      {
        return Ok(target);
      }
      // Flat DS addressing is known; runtime register values are not.
      let address =
        instruction.virtual_address(0, 0, |register, _, _| (register == Register::DS).then(|| 0));
      let slot = match address {
        Some(slot) => slot,
        None => return Ok(target),
      };
      util::read_unaligned(slot as *const usize)? as *const ()
    };
  }
}

/// Decode one instruction without reading past its end.
unsafe fn decode_instruction(address: *const u8) -> Result<Instruction> {
  let mut bytes = [0; 15]; // Maximum x86 instruction length.
  for index in 0..bytes.len() {
    bytes[index] = util::read_unaligned(address.wrapping_add(index))?;
    let mut decoder = Decoder::with_ip(
      usize::BITS,
      &bytes[..=index],
      address as u64,
      DecoderOptions::NONE,
    );
    let instruction = decoder.decode();
    match decoder.last_error() {
      DecoderError::None => return Ok(instruction),
      DecoderError::NoMoreBytes => {},
      _ => return Err(Error::InvalidCode),
    }
  }
  Err(Error::InvalidCode)
}

#[cfg(test)]
mod tests {
  use super::follow_entry_jumps;
  use crate::error::{Error, Result};
  use crate::RawDetour;
  use matches::assert_matches;
  use region::Protection;
  use std::mem;

  #[test]
  fn rejects_cycles_and_unreadable_memory() -> Result<()> {
    let page = region::page::size();
    let mut memory = region::alloc(page * 2, Protection::READ_WRITE_EXECUTE)?;
    unsafe {
      let bytes = std::slice::from_raw_parts_mut(memory.as_mut_ptr::<u8>(), page);
      let target = bytes.as_ptr().cast();
      // Register, far, and TLS-segment jumps cannot be resolved here.
      let unresolved: [&[u8]; 3] = [
        &[0xFF, 0xE0],
        &[0xFF, 0x2D, 0, 0, 0, 0],
        &[0x64, 0xFF, 0x25, 0, 0, 0, 0],
      ];
      for code in unresolved {
        bytes[..code.len()].copy_from_slice(code);
        assert_eq!(follow_entry_jumps(target)?, target);
      }
      bytes[..2].copy_from_slice(&[0xEB, 14]);
      bytes[16..18].copy_from_slice(&[0xEB, 0xEE]);
      assert_matches!(follow_entry_jumps(target), Err(Error::InvalidCode));

      let unreadable = bytes.as_ptr().add(page);
      region::protect(unreadable, page, Protection::NONE)?;
      assert!(follow_entry_jumps(unreadable.cast()).is_err());
      assert!(RawDetour::new(unreadable.cast(), follow_entry_jumps as *const ()).is_err());
      // Both an operand and an indirect slot can straddle an unreadable page.
      bytes[page - 1] = 0xE9;
      assert!(follow_entry_jumps(bytes.as_ptr().add(page - 1).cast()).is_err());
      bytes[..2].copy_from_slice(&[0xFF, 0x25]);
      let slot = if cfg!(target_arch = "x86_64") {
        (page - 4 - 6) as u32
      } else {
        bytes.as_ptr().add(page - 2) as u32
      };
      bytes[2..6].copy_from_slice(&slot.to_le_bytes());
      assert!(follow_entry_jumps(target).is_err());
    }
    Ok(())
  }

  #[test]
  fn hooks_an_existing_entry_detour_and_restores_it() -> Result<()> {
    let mut bytes = crate::arch::memory::POOL
      .lock()
      .unwrap()
      .allocate(follow_entry_jumps as *const (), 256)?;
    unsafe {
      bytes.fill(0x90);
      let previous = bytes.as_ptr().add(64);
      // 0 -> 32 -> 16 -> 64: rel32, negative rel8, then indirect via slot 8.
      bytes[..5].copy_from_slice(&[0xE9, 27, 0, 0, 0]);
      bytes[32..34].copy_from_slice(&[0xEB, 0xEE]);
      bytes[16..18].copy_from_slice(&[0xFF, 0x25]);
      let slot = if cfg!(target_arch = "x86_64") {
        (-14i32) as u32
      } else {
        bytes.as_ptr().add(8) as u32
      };
      bytes[18..22].copy_from_slice(&slot.to_le_bytes());
      let pointer = (previous as usize).to_le_bytes();
      bytes[8..8 + pointer.len()].copy_from_slice(&pointer);
      bytes[64..70].copy_from_slice(&[0xB8, 2, 0, 0, 0, 0xC3]);
      bytes[128..134].copy_from_slice(&[0xB8, 3, 0, 0, 0, 0xC3]);
      assert_eq!(follow_entry_jumps(bytes.as_ptr().cast())?, previous.cast());
      let entry: unsafe extern "C" fn() -> i32 = mem::transmute(bytes.as_ptr());
      let hook = RawDetour::new(bytes.as_ptr().cast(), bytes.as_ptr().add(128).cast())?;
      let original: unsafe extern "C" fn() -> i32 = mem::transmute(hook.trampoline());
      assert_eq!(entry(), 2);
      hook.enable()?;
      assert_eq!(entry(), 3);
      assert_eq!(original(), 2);
      hook.disable()?;
      assert_eq!(entry(), 2);
    }
    Ok(())
  }
}
