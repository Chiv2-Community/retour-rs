use crate::error::{Error, Result};

/// Returns true if an address is executable.
pub fn is_executable_address(address: *const ()) -> Result<bool> {
  Ok(
    region::query(address as *const _)?
      .protection()
      .contains(region::Protection::EXECUTE),
  )
}

// Check every byte, including reads crossing regions. The caller must keep
// mappings stable and ensure the bytes represent a valid T.
pub unsafe fn read_unaligned<T: Copy>(address: *const T) -> Result<T> {
  let mut cursor = address as usize;
  let end = cursor
    .checked_add(std::mem::size_of::<T>())
    .ok_or(Error::InvalidCode)?;
  while cursor < end {
    let region = region::query(cursor as *const u8)?;
    if !region.protection().contains(region::Protection::READ) {
      return Err(Error::InvalidCode);
    }

    cursor = region.as_range().end;
  }
  Ok(std::ptr::read_unaligned(address))
}
