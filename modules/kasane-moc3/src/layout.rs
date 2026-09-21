use kasane_core::types::Status;

use crate::schema::SCHEMA;

pub fn checked(n: usize, field: &str) -> Result<i32, Status> {
    if n > i32::MAX as usize {
        Err(Status::error(
            "CAPACITY",
            format!("{}: exceeds signed 32-bit count", field),
        ))
    } else {
        Ok(n as i32)
    }
}

pub struct Layout {
    pub counts: [u32; 64],
    pub data: Vec<Vec<u8>>,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            counts: [0; 64],
            data: vec![Vec::new(); SCHEMA.len()],
        }
    }

    pub fn field_index(&self, name: &str) -> Result<usize, Status> {
        SCHEMA
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| Status::error("CODEC_LAYOUT", format!("Unknown MOC3 field: {name}")))
    }

    pub fn field(&mut self, name: &str) -> Result<&mut Vec<u8>, Status> {
        let idx = self.field_index(name)?;
        Ok(&mut self.data[idx])
    }

    pub fn integer(&mut self, name: &str, v: i32) -> Result<(), Status> {
        let buf = self.field(name)?;
        buf.extend_from_slice(&(v as u32).to_le_bytes());
        Ok(())
    }

    pub fn scalar(&mut self, name: &str, v: f32) -> Result<(), Status> {
        let buf = self.field(name)?;
        buf.extend_from_slice(&v.to_le_bytes());
        Ok(())
    }

    pub fn short(&mut self, name: &str, v: u16) -> Result<(), Status> {
        let buf = self.field(name)?;
        buf.extend_from_slice(&v.to_le_bytes());
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Status> {
        // Write the 64 counts into count_info (section 0)
        self.data[0].clear();
        for &n in &self.counts {
            self.data[0].extend_from_slice(&n.to_le_bytes());
        }

        // Reserve loader scratch after the 64-byte header and 160 offsets.
        // Both Core implementations revive pointers in this area in-place.
        // This is zeroed wire padding, never a serialized native structure.
        let mut out = vec![0u8; 1984];
        out[0] = b'M';
        out[1] = b'O';
        out[2] = b'C';
        out[3] = b'3';
        out[4] = 5;

        for (i, s) in SCHEMA.iter().enumerate() {
            let count = if s.count_index < 0 {
                1
            } else {
                self.counts[s.count_index as usize] as usize
            };
            let expected = count * s.width;

            // Only loader-owned runtime pointer slots may be synthesized.
            if s.name.ends_with("_runtime") {
                self.data[i].resize(expected, 0);
            }

            if self.data[i].len() != expected {
                return Err(Status::error(
                    "CODEC_LAYOUT",
                    format!("{}: schema size mismatch", s.name),
                ));
            }

            let aligned = (out.len() + 63) & !63;
            out.resize(aligned, 0);

            if out.len() > i32::MAX as usize
                || expected > (i32::MAX as usize).saturating_sub(out.len())
            {
                return Err(Status::error(
                    "CAPACITY",
                    format!("{}: exceeds Core signed 32-bit offset limit", s.name),
                ));
            }

            let offset = out.len() as u32;
            let patch_pos = 64 + i * 4;
            out[patch_pos..patch_pos + 4].copy_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&self.data[i]);
        }

        let aligned = (out.len() + 63) & !63;
        out.resize(aligned, 0);
        Ok(out)
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}
