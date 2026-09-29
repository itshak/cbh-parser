//! One game's decoded `moves2` and the shape of its variation tree.
//!
//! [`MovesBuf`] is a caller-owned buffer: one reused across games keeps
//! decoding allocation-free after it has grown to the size it needs. It
//! implements [`MoveSink`], so [`decode_game_into`] fills it; the slices it
//! hands out describe the tree in the stored order of the format (depth first,
//! the main line first at every position):
//!
//! - [`MovesBuf::moves`] is the `moves2` stream ([`NULL_MOVE`] for a null move),
//! - [`MovesBuf::parents`] names, per move, the move it follows, or [`ROOT`]
//!   for a move played from the start position,
//! - [`MovesBuf::is_main`] says, per move, whether it belongs to the game's
//!   main line.
//!
//! A caller can rebuild the tree from those three slices alone.

use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::error::Result;
use gigachess::{Board, Move};

use super::decode::{GameRef, MoveSink, TreeStats, start_as_played, walk_from};
use super::start::start_board;

/// The parent of a move played from the start position.
pub const ROOT: u32 = u32::MAX;

/// A buffer of one game's decoded moves and tree structure.
#[derive(Clone, Debug, Default)]
pub struct MovesBuf {
    start: Option<Board>,
    moves: Vec<u16>,
    parents: Vec<u32>,
    main: Vec<bool>,
    stats: TreeStats,
    /// The move the next move follows, set by [`MoveSink::branch`] and
    /// [`MoveSink::resume`]; `None` continues from the last move.
    next_parent: Option<u32>,
    /// The moves a pending variation hangs from, innermost last: the next move
    /// after a line ends follows the move a branch was opened at.
    branches: Vec<u32>,
}

impl MovesBuf {
    /// An empty buffer with room for `capacity` moves.
    pub fn with_capacity(capacity: usize) -> MovesBuf {
        MovesBuf {
            moves: Vec::with_capacity(capacity),
            parents: Vec::with_capacity(capacity),
            main: Vec::with_capacity(capacity),
            ..Default::default()
        }
    }

    /// Empties the buffer, keeping its memory.
    pub fn clear(&mut self) {
        self.start = None;
        self.moves.clear();
        self.parents.clear();
        self.main.clear();
        self.stats = TreeStats::default();
        self.next_parent = None;
        self.branches.clear();
    }

    /// The position the game starts from, once decoded.
    pub fn start(&self) -> Option<Board> {
        self.start
    }

    /// The `moves2` stream in the stored order of the format.
    pub fn moves(&self) -> &[u16] {
        &self.moves
    }

    /// Per move, the index of the move it follows, or [`ROOT`].
    pub fn parents(&self) -> &[u32] {
        &self.parents
    }

    /// Per move, whether it belongs to the game's main line.
    pub fn is_main(&self) -> &[bool] {
        &self.main
    }

    /// What the decode counted.
    pub fn stats(&self) -> TreeStats {
        self.stats
    }

    /// Moves decoded (a variation tree's whole ply count).
    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// Whether no move was decoded.
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Whether move `i` is a pass.
    pub fn is_null(&self, i: usize) -> bool {
        self.moves.get(i).is_some_and(|w| Move::from_word(*w).is_null())
    }
}

impl MoveSink for MovesBuf {
    fn play(&mut self, _board: &Board, mv: u16, main: bool) {
        let last = self.moves.len().checked_sub(1).map(|i| i as u32);
        let parent = self.next_parent.or(last).unwrap_or(ROOT);
        self.moves.push(mv);
        self.parents.push(parent);
        self.main.push(main);
        self.next_parent = None;
    }

    fn played(&mut self, _board: &Board) {}

    fn branch(&mut self) {
        // The move just reported opened a variation: the moves that follow are
        // played from the position after it, so they are its children, and the
        // move itself is what a later `resume` returns to.
        self.branches.push(self.moves.len().saturating_sub(1) as u32);
    }

    fn resume(&mut self) {
        // The line of that move's variation ended: the moves that follow are
        // its siblings, played from the same position the move was.
        let opened = self.branches.pop().unwrap_or(0);
        let parent = self.parents.get(opened as usize).copied().unwrap_or(ROOT);
        self.next_parent = Some(parent);
    }
}

/// Decodes `game` into `buf` (which is emptied first) and returns what the
/// walk counted.
pub fn decode_game_into(what: GameRef, game: &GameMoves<'_>, buf: &mut MovesBuf) -> Result<TreeStats> {
    let start = start_as_played(what, game)?;
    let board = start_board(&start)?;
    buf.clear();
    buf.start = Some(board);
    let stats = walk_from(what, game, &start, buf)?;
    buf.stats = stats;
    Ok(stats)
}
