use std::{io::Write, path::Path};
pub struct Staging {
    pub chunks: usize,
    pub max_chunk_bytes: usize,
    expected: Vec<usize>,
    received: [Vec<usize>; 2],
}
impl Staging {
    pub fn new(expected: Vec<usize>) -> Self {
        Self {
            chunks: 0,
            max_chunk_bytes: 0,
            received: [vec![0; expected.len()], vec![0; expected.len()]],
            expected,
        }
    }
    pub fn primary_complete(&self) -> bool {
        self.received[0] == self.expected
    }
    pub fn alternate_complete(&self) -> bool {
        self.received[1]
            .iter()
            .zip(&self.expected)
            .all(|(&n, &e)| n == 0 || n == e)
    }
    pub fn any_alternate(&self) -> bool {
        self.received[1].iter().any(|&n| n > 0)
    }
    pub fn alternate_pages(&self) -> usize {
        self.received[1].iter().filter(|&&n| n > 0).count()
    }
    pub fn has_alternate(&self, index: usize) -> bool {
        self.received[1][index] > 0
    }
    pub fn write(
        &mut self,
        directory: &Path,
        index: usize,
        alternate: bool,
        offset: usize,
        bytes: &[u8],
        chunked: bool,
    ) -> Result<(), String> {
        let variant = usize::from(alternate);
        let expected = *self.expected.get(index).ok_or("页面编号超过原件范围")?;
        if alternate && self.received[0][index] != expected {
            return Err("页面渲染尚未完成".into());
        }
        let received = &mut self.received[variant];
        if received[..index]
            .iter()
            .zip(&self.expected)
            .any(|(&n, &e)| if alternate { n != 0 && n != e } else { n != e })
            || received[index] != offset
            || bytes.is_empty()
            || bytes.len() > expected.saturating_sub(offset)
            || (chunked && bytes.len() > 1024 * 1024)
        {
            return Err("页面像素长度与尺寸不符".into());
        }
        let path = if alternate {
            let p = directory.join("supersampled");
            std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
            p
        } else {
            directory.to_owned()
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(offset == 0)
            .append(offset != 0)
            .open(path.join(format!("{index}.rgb")))
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        received[index] += bytes.len();
        self.chunks += 1;
        self.max_chunk_bytes = self.max_chunk_bytes.max(bytes.len());
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_are_ordered_bounded_and_alternate_pages_can_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Staging::new(vec![6, 3]);
        assert!(s.write(dir.path(), 1, false, 0, &[1, 2, 3], true).is_err());
        s.write(dir.path(), 0, false, 0, &[1, 2, 3], true).unwrap();
        assert!(!s.primary_complete());
        assert!(s.write(dir.path(), 0, false, 0, &[4, 5, 6], true).is_err());
        s.write(dir.path(), 0, false, 3, &[4, 5, 6], true).unwrap();
        s.write(dir.path(), 1, false, 0, &[7, 8, 9], true).unwrap();
        assert!(s.primary_complete());
        s.write(dir.path(), 1, true, 0, &[9, 8, 7], true).unwrap();
        assert!(s.any_alternate() && s.alternate_complete());
        assert_eq!(s.alternate_pages(), 1);
        assert!(!s.has_alternate(0));
        assert_eq!(
            std::fs::read(dir.path().join("0.rgb")).unwrap(),
            vec![1, 2, 3, 4, 5, 6]
        );
    }
    #[test]
    fn incomplete_alternate_or_duplicate_page_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Staging::new(vec![6]);
        assert!(s.write(dir.path(), 0, true, 0, &[1, 2], true).is_err());
        s.write(dir.path(), 0, false, 0, &[0; 6], true).unwrap();
        s.write(dir.path(), 0, true, 0, &[1, 2], true).unwrap();
        assert!(!s.alternate_complete());
        assert!(s.write(dir.path(), 0, true, 3, &[1], true).is_err());
    }
}
