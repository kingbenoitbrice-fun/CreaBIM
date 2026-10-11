//! Whitespace tokenizer for OpenCascade's text BREP format.

use crate::FcError;

/// Cursor over the whitespace-separated tokens of a BREP text.
pub struct Tokens<'a> {
    toks: Vec<&'a str>,
    pos: usize,
}

impl<'a> Tokens<'a> {
    pub fn new(text: &'a str) -> Self {
        Tokens {
            toks: text.split_ascii_whitespace().collect(),
            pos: 0,
        }
    }

    pub fn peek(&self) -> Option<&'a str> {
        self.toks.get(self.pos).copied()
    }

    pub fn next(&mut self) -> Result<&'a str, FcError> {
        let t = self
            .toks
            .get(self.pos)
            .copied()
            .ok_or_else(|| FcError::Brep("unexpected end of BREP data".into()))?;
        self.pos += 1;
        Ok(t)
    }

    /// Moves just past the next occurrence of `word`.
    pub fn seek(&mut self, word: &str) -> Result<(), FcError> {
        while self.next()? != word {}
        Ok(())
    }

    pub fn expect(&mut self, word: &str) -> Result<(), FcError> {
        let t = self.next()?;
        if t == word {
            Ok(())
        } else {
            Err(FcError::Brep(format!("expected '{word}', found '{t}'")))
        }
    }

    pub fn f64(&mut self) -> Result<f64, FcError> {
        let t = self.next()?;
        t.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| FcError::Brep(format!("bad number '{t}'")))
    }

    pub fn int(&mut self) -> Result<i64, FcError> {
        let t = self.next()?;
        t.parse::<i64>()
            .map_err(|_| FcError::Brep(format!("bad integer '{t}'")))
    }

    /// A count or index; rejects negatives and absurd sizes up front so a
    /// corrupted header cannot trigger a huge allocation.
    pub fn count(&mut self) -> Result<usize, FcError> {
        let v = self.int()?;
        let left = self.toks.len().saturating_sub(self.pos);
        if v < 0 || v as u64 > (left as u64 + 1) * 4 + 16 {
            return Err(FcError::Brep(format!("implausible count {v}")));
        }
        Ok(v as usize)
    }

    /// An integer that may have a regularity suffix glued to it, as in
    /// `3  1 2CN 1 0 ...` (OpenCascade prints "CN" right after the index).
    pub fn int_with_suffix(&mut self) -> Result<(i64, Option<&'a str>), FcError> {
        let t = self.next()?;
        let split = t
            .char_indices()
            .find(|(i, c)| *i > 0 && c.is_ascii_alphabetic())
            .map(|(i, _)| i);
        match split {
            Some(i) => {
                let n = t[..i]
                    .parse::<i64>()
                    .map_err(|_| FcError::Brep(format!("bad integer '{t}'")))?;
                Ok((n, Some(&t[i..])))
            }
            None => {
                let n = t
                    .parse::<i64>()
                    .map_err(|_| FcError::Brep(format!("bad integer '{t}'")))?;
                Ok((n, None))
            }
        }
    }

    pub fn v3(&mut self) -> Result<crate::math::V3, FcError> {
        Ok(crate::math::v3(self.f64()?, self.f64()?, self.f64()?))
    }

    pub fn v2(&mut self) -> Result<crate::math::V2, FcError> {
        Ok(crate::math::v2(self.f64()?, self.f64()?))
    }
}
