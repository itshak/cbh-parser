//! Probe (throwaway): what the export's stages cost on the real database.
//!
//! Four passes over the same records, each adding one stage: the record
//! plumbing (header, record bytes, `GameMoves::parse`, the start), the walk,
//! the full writer (tags, movetext, annotations), and the writer with the
//! annotations off. The differences are the stages.
#![allow(missing_docs)]

use std::path::Path;
use std::time::Instant;

use cbvault::pgn::PgnWriter;
use cbvault_chess::decode::{GameRef, MoveSink, start_as_played};
use cbvault_chess::start::StartCache;
use cbvault_chess::start::start_board_cached;
use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::cbh::{Annotations, Entities, Headers, Wide};
use cbvault_format::file::DbFile;
use gigachess::Board;

struct Sink(u64);
impl MoveSink for Sink {
    fn play(&mut self, _before: &Board, _mv: u16, _main: bool) {
        self.0 += 1;
    }
    fn played(&mut self, _after: &Board) {}
    fn branch(&mut self) {}
    fn resume(&mut self) {}
    fn wants_checkers(&self) -> bool {
        true
    }
}

struct Count(u64);
impl std::io::Write for Count {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = Path::new(&args[1]);
    let last: u32 = args[2].parse().expect("last record");

    let headers = Headers::open(base).expect("headers");
    let entities = Entities::open(base).expect("entities");
    let annotations = Annotations::open(base).expect("cba");
    let wide = Wide::open(base).ok();
    let cbg_path = base.with_extension("cbg");
    let cbg = DbFile::open(cbg_path.clone()).expect("cbg");
    let mut rec = Vec::new();
    let mut ann_scratch: Vec<u8> = Vec::new();

    // stage 1: the record plumbing only
    let t = Instant::now();
    for id in 1..=last.min(headers.records()) {
        let Ok(header) = headers.record(id) else { continue };
        if header.is_deleted() {
            continue;
        }
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        cbg.read_into(at, &mut head).expect("head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        rec.clear();
        rec.resize(size, 0);
        cbg.read_into(at, rec.as_mut_slice()).expect("record");
        let game = GameMoves::parse(&cbg_path, &rec).expect("record");
        let start = start_as_played(GameRef::new(id), &game).expect("start");
        let mut cache = StartCache::new();
        start_board_cached(&start, &mut cache).expect("board");
    }
    let t_plumbing = t.elapsed().as_secs_f64();

    // stage 2: + the walk
    let t = Instant::now();
    let mut sink = Sink(0);
    for id in 1..=last.min(headers.records()) {
        let Ok(header) = headers.record(id) else { continue };
        if header.is_deleted() {
            continue;
        }
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        cbg.read_into(at, &mut head).expect("head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        rec.clear();
        rec.resize(size, 0);
        cbg.read_into(at, rec.as_mut_slice()).expect("record");
        let Ok(game) = GameMoves::parse(&cbg_path, &rec) else { continue };
        let Ok(start) = start_as_played(GameRef::new(id), &game) else { continue };
        let _ = cbvault_chess::decode::walk_from(GameRef::new(id), &game, &start, &mut sink);
    }
    let t_walk = t.elapsed().as_secs_f64();

    // stage 3: + the full writer, annotations on
    let (t_full, bytes) =
        export(base, &headers, &entities, &annotations, wide.as_ref(), &cbg_path, last, true, &mut ann_scratch);
    // stage 4: the same with the annotations off
    let (t_no_ann, _) =
        export(base, &headers, &entities, &annotations, wide.as_ref(), &cbg_path, last, false, &mut ann_scratch);

    println!("records {last}, moves walked {}", sink.0);
    println!("1. record plumbing (header, record, parse, start)      {t_plumbing:6.2} s");
    println!(
        "2. + the walk (legality, SAN, the sink)                  {t_walk:6.2} s   (stage 2 - 1 = {:.2})",
        t_walk - t_plumbing
    );
    println!("3. the full writer, annotations on                       {t_full:6.2} s   ({bytes} bytes)");
    println!("4. the full writer, annotations off                      {t_no_ann:6.2} s");
    println!();
    println!("   of which the annotations cost                         {:6.2} s", t_full - t_no_ann);
    println!("   so the writer's own text (tags, movetext, output)     {:6.2} s", t_no_ann - t_walk);
    println!("   and the whole export is                               {:6.2} s", t_full);
}

#[allow(clippy::too_many_arguments)]
fn export(
    base: &Path,
    headers: &Headers,
    entities: &Entities,
    annotations: &Annotations,
    wide: Option<&Wide>,
    cbg_path: &Path,
    last: u32,
    with_anns: bool,
    ann_scratch: &mut Vec<u8>,
) -> (f64, u64) {
    let cbg = DbFile::open(cbg_path.to_path_buf()).expect("cbg");
    let mut writer = PgnWriter::new();
    let mut out = Count(0);
    let mut rec = Vec::new();
    let t = Instant::now();
    for id in 1..=last.min(headers.records()) {
        let Ok(header) = headers.record(id) else { continue };
        if header.is_deleted() {
            continue;
        }
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        cbg.read_into(at, &mut head).expect("head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        rec.clear();
        rec.resize(size, 0);
        cbg.read_into(at, rec.as_mut_slice()).expect("record");
        let Ok(game) = GameMoves::parse(cbg_path, &rec) else { continue };
        let anns = if with_anns { annotations.of(&header, wide, ann_scratch).ok() } else { None };
        let _ = writer.write_game(&mut out, &header, entities, &game, anns.as_ref());
    }
    let secs = t.elapsed().as_secs_f64();
    let _ = base;
    (secs, out.0)
}
