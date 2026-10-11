use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Stable identity of an element. Survives save/load and maps 1:1 to an IFC GlobalId.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ElementId(pub Uuid);

impl ElementId {
    /// The id of no element, for drawing items that belong to none.
    pub const NIL: ElementId = ElementId(Uuid::nil());

    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// The 22-character compressed form IFC uses for `GlobalId`.
    pub fn ifc_guid(&self) -> String {
        ifc_guid(self.0)
    }

    /// The inverse of [`ElementId::ifc_guid`], so an imported element keeps
    /// the identity it had in the IFC file. `None` if `guid` is not a valid
    /// 22-character IFC GlobalId.
    pub fn from_ifc_guid(guid: &str) -> Option<Self> {
        from_ifc_guid(guid).map(Self)
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

/// Decode an IFC GlobalId back into the UUID it encodes.
pub fn from_ifc_guid(guid: &str) -> Option<Uuid> {
    let bytes = guid.as_bytes();
    if bytes.len() != 22 {
        return None;
    }
    let digit = |c: u8| IFC_CHARS.iter().position(|&x| x == c).map(|d| d as u128);
    let first = digit(bytes[0])?;
    if first > 3 {
        return None; // only 2 bits fit before the remaining 126
    }
    let mut n = first;
    for &c in &bytes[1..] {
        n = (n << 6) | digit(c)?;
    }
    Some(Uuid::from_u128(n))
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

    #[test]
    fn ifc_guid_round_trips() {
        for _ in 0..100 {
            let id = ElementId::new();
            assert_eq!(ElementId::from_ifc_guid(&id.ifc_guid()), Some(id));
        }
        assert_eq!(from_ifc_guid("0000000000000000000000"), Some(Uuid::nil()));
        assert_eq!(from_ifc_guid("3$$$$$$$$$$$$$$$$$$$$$"), Some(Uuid::max()));
    }

    #[test]
    fn invalid_ifc_guids_are_rejected() {
        assert_eq!(from_ifc_guid(""), None);
        assert_eq!(from_ifc_guid("4000000000000000000000"), None);
        assert_eq!(from_ifc_guid("000000000000000000000!"), None);
        assert_eq!(from_ifc_guid("00000000000000000000000"), None);
    }
}
