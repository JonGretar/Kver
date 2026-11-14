use super::*;

/// A location in the PDF's native coordinate system, independent of extraction
/// order, page rotation, zoom, and ordinary viewport restoration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RulerAnchor {
    pub(crate) page: usize,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl RulerAnchor {
    pub(crate) fn is_valid(self) -> bool {
        i64::try_from(self.page).is_ok() && self.x.is_finite() && self.y.is_finite()
    }
}

impl RecentStore {
    pub(crate) fn ruler_anchor(&self, path: &Path) -> Result<Option<RulerAnchor>, String> {
        if !self.database_exists()? {
            return Ok(None);
        }
        let stored = self
            .connect()?
            .query_row(
                "SELECT page, x, y FROM ruler_anchors WHERE path = ?1",
                [path
                    .to_str()
                    .ok_or("Cannot read a non-UTF-8 document path")?],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(stored.and_then(|(page, x, y)| {
            let anchor = RulerAnchor {
                page: usize::try_from(page).ok()?,
                x,
                y,
            };
            anchor.is_valid().then_some(anchor)
        }))
    }

    pub(crate) fn save_ruler_anchor(&self, path: &Path, anchor: RulerAnchor) -> Result<(), String> {
        if !anchor.is_valid() {
            return Err("Invalid reading ruler location".into());
        }
        self.connect()?
            .execute(
                "INSERT INTO ruler_anchors(path, page, x, y)
             SELECT path, ?2, ?3, ?4 FROM documents WHERE path = ?1
             ON CONFLICT(path) DO UPDATE SET page = excluded.page, x = excluded.x, y = excluded.y",
                params![
                    path.to_str()
                        .ok_or("Cannot store a non-UTF-8 document path")?,
                    anchor.page as i64,
                    anchor.x,
                    anchor.y
                ],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
