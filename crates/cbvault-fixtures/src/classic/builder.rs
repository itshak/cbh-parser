//! [`Builder`]: the files of a small classic database.
//!
//! Ported from `cbformat`'s `fixture_cbh/builder.rs` (MIT, `oschess-cb-bridge`
//! @ `ca9e8f8e`); see `docs/provenance.md`.

use crate::TempDb;

/// Builds a classic database: game and guiding text records, their move,
/// text and annotation records in the order added, and the entities. It
/// starts with two players (`Morphy`, `Anderssen`), one tournament (`Paris`),
/// and one empty annotator and source; each entity added gets the next id.
pub struct Builder {
    records: Vec<[u8; 46]>,
    cbg: Vec<u8>,
    cba: Vec<u8>,
    players: Vec<Vec<u8>>,
    tournaments: Vec<Vec<u8>>,
    annotators: Vec<Vec<u8>>,
    sources: Vec<Vec<u8>>,
}

impl Default for Builder {
    fn default() -> Self {
        let mut cbg = vec![0u8; 26];
        cbg[1] = 26;
        Builder {
            records: Vec::new(),
            cbg,
            cba: vec![0u8; 26],
            players: vec![b"Morphy".to_vec(), b"Anderssen".to_vec()],
            tournaments: vec![b"Paris".to_vec()],
            annotators: vec![Vec::new()],
            sources: vec![Vec::new()],
        }
    }
}

/// `fields` written into a record of `size` bytes, each at its offset and cut
/// to its width.
fn data(size: usize, fields: &[(usize, usize, &[u8])]) -> Vec<u8> {
    let mut d = vec![0u8; size];
    for &(at, width, bytes) in fields {
        let n = bytes.len().min(width);
        d[at..at + n].copy_from_slice(&bytes[..n]);
    }
    d
}

impl Builder {
    /// A builder with the default entities.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a game, won by white between players 0 and 1, with move
    /// record `rec`, and returns its header record for further changes.
    pub fn game(&mut self, rec: &[u8]) -> &mut [u8; 46] {
        let mut h = [0u8; 46];
        h[0] = 1;
        h[1..5].copy_from_slice(&(self.cbg.len() as u32).to_be_bytes());
        h[0x0c..0x0f].copy_from_slice(&[0, 0, 1]);
        h[0x1b] = 2;
        self.cbg.extend(rec);
        self.records.push(h);
        self.records.last_mut().unwrap()
    }

    /// Appends a guiding text with one title per `(language, title)` and no
    /// content, and returns its header record for further changes.
    pub fn text(&mut self, titles: &[(u16, &[u8])]) -> &mut [u8; 46] {
        let mut r = vec![0x80, 0, 0, 0];
        r.extend(1u16.to_le_bytes());
        r.extend((titles.len() as u16).to_le_bytes());
        for (language, title) in titles {
            r.extend(language.to_le_bytes());
            r.extend((title.len() as u16).to_le_bytes());
            r.extend(*title);
        }
        r.extend([0, 0, 0]);
        let size = (r.len() as u32).to_be_bytes();
        r[1..4].copy_from_slice(&size[1..]);
        let mut h = [0u8; 46];
        h[0] = 3;
        h[1..5].copy_from_slice(&(self.cbg.len() as u32).to_be_bytes());
        self.cbg.extend(r);
        self.records.push(h);
        self.records.last_mut().unwrap()
    }

    /// Gives the last game added the annotation record `rec`.
    pub fn annotations(&mut self, rec: &[u8]) -> &mut Self {
        let at = (self.cba.len() as u32).to_be_bytes();
        self.records.last_mut().expect("a game first")[5..9].copy_from_slice(&at);
        self.cba.extend(rec);
        self
    }

    /// Adds a player and returns its id.
    pub fn player(&mut self, last: &str, first: &str) -> u32 {
        self.players.push(data(50, &[(0, 30, last.as_bytes()), (30, 20, first.as_bytes())]));
        self.players.len() as u32 - 1
    }

    /// Adds a tournament and returns its id.
    pub fn tournament(&mut self, title: &str, place: &str) -> u32 {
        self.tournaments.push(data(70, &[(0, 40, title.as_bytes()), (40, 30, place.as_bytes())]));
        self.tournaments.len() as u32 - 1
    }

    /// Adds an annotator and returns its id.
    pub fn annotator(&mut self, name: &str) -> u32 {
        self.annotators.push(data(45, &[(0, 45, name.as_bytes())]));
        self.annotators.len() as u32 - 1
    }

    /// Writes an entity file whose records form a **balanced tree over their name
    /// fields**, as a real namebase does.
    ///
    /// The obvious encoding — every child link `-1` — writes a file that is a
    /// single root with no descendants, and a reader that walks the tree then
    /// finds exactly one record no matter how many exist. That is a fixture that
    /// cannot express the case a test needs to make, so nothing ever notices.
    /// Sorting the records by name field and linking them as a balanced tree from
    /// that order gives a file whose descent and whose walk agree with a real
    /// one, duplicates included.
    ///
    /// `name_width` is where each record's sorted key ends.
    fn entity_file(records: &[Vec<u8>], data: usize, name_width: usize) -> Vec<u8> {
        let key = |r: &Vec<u8>| {
            let end = name_width.min(r.len());
            let stop = r[..end].iter().position(|&b| b == 0).unwrap_or(end);
            r[..stop].to_vec()
        };
        let mut order: Vec<usize> = (0..records.len()).collect();
        // A stable sort, so two records with the same key keep their id order and
        // the tree's in-order walk is ascending by id, as a real file's is.
        order.sort_by(|&a, &b| key(&records[a]).cmp(&key(&records[b])).then(a.cmp(&b)));

        // left and right per record index, -1 for none.
        let mut left = vec![-1i32; records.len()];
        let mut right = vec![-1i32; records.len()];
        // Recursion over a sub-range of `order`, taking its midpoint as the root.
        // `order` is sorted, so the result is a balanced search tree whose in-order
        // walk is ascending — the shape and the ordering a real namebase has,
        // which is what makes a descent and a walk agree on the same answer.
        //
        // Written with an explicit `(lo, hi)` walk rather than a recursive helper
        // over two aliased `&mut` slices: the recursive version indexed the
        // slices by `order[mid]` on the way in and by the returned node id on the
        // way out, and the two are not the same index, so a node could be written
        // twice and end up linked to itself.
        let mut stack: Vec<(usize, usize)> = vec![(0, order.len())];
        while let Some((lo, hi)) = stack.pop() {
            if lo >= hi {
                continue;
            }
            let mid = lo + (hi - lo) / 2;
            let node = order[mid];
            left[node] = if lo < mid { order[(lo + mid) / 2] as i32 } else { -1 };
            right[node] = if mid + 1 < hi { order[mid + 1 + (hi - mid - 1) / 2] as i32 } else { -1 };
            stack.push((lo, mid));
            stack.push((mid + 1, hi));
        }
        let root = order.get(order.len() / 2).map_or(-1, |&i| i as i32);

        let mut f = Vec::new();
        // The header is seven 32-bit fields, and their offsets are fixed by the
        // format: count at 0x00, **root at 0x04**, the magic at 0x08, the record
        // data size at 0x0c, and the extra-header-bytes count at 0x18. Root being
        // the *second* field, not the fifth, is what the reader reads; writing it
        // anywhere else leaves the reader's root at 0, which is a valid-looking
        // leaf — so every namebase lookup silently searched the first record and
        // nothing noticed, because record 0 is often the right answer.
        for v in [records.len() as i32, root, 1_234_567_890, data as i32, 0, records.len() as i32, 0] {
            f.extend(v.to_le_bytes());
        }
        for (i, r) in records.iter().enumerate() {
            f.extend(left[i].to_le_bytes());
            f.extend(right[i].to_le_bytes());
            f.push(0);
            let mut d = r.clone();
            d.resize(data, 0);
            f.extend(d);
        }
        f
    }

    /// Writes `db.cbh` and its companions to a new temporary directory.
    pub fn write(&self, name: &str) -> TempDb {
        let db = TempDb::create(name);
        let mut cbh = vec![0u8; 46];
        cbh[1..6].copy_from_slice(&[0, 44, 0, 46, 1]);
        cbh[6..10].copy_from_slice(&(self.records.len() as u32 + 1).to_be_bytes());
        for r in &self.records {
            cbh.extend(r);
        }
        let mut cbg = self.cbg.clone();
        let size = (cbg.len() as u32).to_be_bytes();
        cbg[2..6].copy_from_slice(&size);
        let entity = |data: usize, recs: &[Vec<u8>], name_width: usize| Self::entity_file(recs, data, name_width);
        db.write(".cbh", &cbh);
        db.write(".cbg", &cbg);
        db.write(".cba", &self.cba);
        db.write(".cbp", &entity(58, &self.players, 30));
        db.write(".cbt", &entity(90, &self.tournaments, 40));
        db.write(".cbc", &entity(53, &self.annotators, 45));
        db.write(".cbs", &entity(59, &self.sources, 25));
        db
    }
}
