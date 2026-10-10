use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Stable identity of an element. Survives save/load and maps 1:1 to an IFC GlobalId.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ElementId(pub Uuid);

impl ElementId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// The 22-character compressed form IFC uses for `GlobalId`.
    pub fn ifc_guid(&self) -> String {
        ifc_guid(self.0)
    }
}

impl Default for ElementId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0.simple().to_string()[..8])
    }
}

impl fmt::Debug for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ElementId({self})")
    }
}

const IFC_CHARS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// Encode a UUID with IFC's base-64 alphabet: 2 bits in the first character,
/// then 6 bits in each of the remaining 21.
pub fn ifc_guid(uuid: Uuid) -> String {
    let n = uuid.as_u128();
    let mut out = String::with_capacity(22);
    out.push(IFC_CHARS[(n >> 126) as usize] as char);
    for i in (0..21).rev() {
        out.push(IFC_CHARS[((n >> (i * 6)) & 0x3f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ifc_guid_is_22_chars_and_first_char_in_range() {
        for _ in 0..100 {
            let g = ElementId::new().ifc_guid();
            assert_eq!(g.len(), 22);
            assert!("0123".contains(g.chars().next().unwrap()));
        }
    }

    #[test]
    fn ifc_guid_known_values() {
        assert_eq!(ifc_guid(Uuid::nil()), "0000000000000000000000");
        assert_eq!(ifc_guid(Uuid::max()), "3$$$$$$$$$$$$$$$$$$$$$");
    }
}
