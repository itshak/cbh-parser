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
        let entity = |data: usize, recs: &[Vec<u8>]| {
            let mut f = Vec::new();
            for v in [recs.len() as i32, 0, 1_234_567_890, data as i32, -1, recs.len() as i32, 0] {
                f.extend(v.to_le_bytes());
            }
            for r in recs {
                f.extend((-1i32).to_le_bytes());
                f.extend((-1i32).to_le_bytes());
                f.push(0);
                let mut d = r.clone();
                d.resize(data, 0);
                f.extend(d);
            }
            f
        };
        db.write(".cbh", &cbh);
        db.write(".cbg", &cbg);
        db.write(".cba", &self.cba);
        db.write(".cbp", &entity(58, &self.players));
        db.write(".cbt", &entity(90, &self.tournaments));
        db.write(".cbc", &entity(53, &self.annotators));
        db.write(".cbs", &entity(59, &self.sources));
        db
    }
}
