use crate::db::{Move, Game, InsertionError};
use crate::queries::{game_from_id, movements_from_game, games_from_player, related_games_from_game};
use cursive::event::{Event, Key, EventResult};
use cursive::theme::{ColorStyle, Color, BaseColor, Style, Effect};
use cursive::view::{Resizable, ScrollStrategy};
use pgn_reader::{Square, Role, Color as PieceColor};
use shakmaty::{Board, Piece, Chess, Position};
use cursive::views::{Dialog, LinearLayout, EditView, TextView, DummyView, Panel, ScrollView, SelectView};
use cursive::traits::Nameable;
use cursive::{Cursive, CursiveExt, View};
use sqlx::{PgConnection, Connection};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct BoardState {
  game: Game,
  moves: Vec<Move>,
  related_games: Vec<Vec<(Move, Game)>>,
  curr_move_idx: usize
}

impl BoardState {
  async fn build(conn: &mut PgConnection, game: Game) -> Result<Self, InsertionError> {
    let moves = movements_from_game(conn, game.id.clone()).await?;
    let related_games = related_games_from_game(conn, game.id.id).await?;
    Ok(BoardState {
      game,
      moves,
      related_games,
      curr_move_idx: 0
    })
  }

  fn game_up_to_move(&self, move_idx: usize) -> Chess {
    let mut chess = Chess::default();
    for movement in self.moves.iter().take(move_idx) {
      let mov = movement.san_plus.0.san.to_move(&chess).expect("invalid move in database");
      chess.play_unchecked(&mov);
    }
    chess
  }

  fn current_board(&self) -> Board {
    let game = self.game_up_to_move(self.curr_move_idx);
    game.board().clone()
  }

  fn last_move_from_square(&self) -> Option<Square> {
    if self.curr_move_idx == 0 {
      return None
    }
    let chess = self.game_up_to_move(self.curr_move_idx - 1);
    self.moves
      .get(self.curr_move_idx - 1)
      .and_then(|mvmt| mvmt.san_plus.0.san.to_move(&chess).expect("invalid move in database").from())
  }
  fn last_move_to_square(&self) -> Option<Square> {
    if self.curr_move_idx == 0 {
      return None
    }
    let chess = self.game_up_to_move(self.curr_move_idx - 1);
    self.moves
      .get(self.curr_move_idx - 1)
      .map(|mvmt| mvmt.san_plus.0.san.to_move(&chess).expect("invalid move in database").to())
  }
}

fn fetch_game(db_url: String, game_id: i32) -> Result<BoardState, InsertionError> {
  let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap();
  rt.block_on(async {
    let mut conn = PgConnection::connect(&db_url).await?;
    let game = game_from_id(&mut conn, game_id).await?;
    let board_state = BoardState::build(&mut conn, game).await?;
    Ok(board_state)
  })
}
fn fetch_games_from_player(db_url: &str, player_name: &str) -> Result<Vec<Game>, InsertionError> {
  let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap();
  rt.block_on(async {
    let mut conn = PgConnection::connect(db_url).await?;
    let games = games_from_player(&mut conn, player_name).await?;
    Ok(games)
  })
}

pub fn cli_entrypoint(db_url: String) {
  let mut siv = cursive::default();
  siv.set_window_title("Nixchess");
  siv.set_user_data(db_url);
  siv.add_global_callback('q', |s| { s.quit(); });
  siv.add_global_callback('\'', Cursive::toggle_debug_console);
  siv.add_layer(player_selector());
  siv.run_crossterm().expect("Could not run on crossterm backend");
}

fn player_selector() -> impl View {
  Dialog::around(EditView::new().with_name("player_name"))
    .title("Player name:")
    .button("Ok", move |s| {
      let player_name = s.call_on_name("player_name", |v: &mut EditView| v.get_content()).unwrap();
      let db_url = s.user_data::<String>().unwrap().clone();
      let games = fetch_games_from_player(&db_url, &player_name);
      match games {
        Ok(games) => {
          s.add_layer(game_selector((*player_name).clone(), games, db_url))
        },
        Err(err) => error_pop_up(s, err),
      };
    })
}

fn game_selector(player_name: String, games: Vec<Game>, db_url: String) -> impl View {
  let mut game_selector = SelectView::new();
  let games_number = games.len();
  for game in games {
    let game_description = if game.white == player_name {
      format!("[W] vs {} - {} @ {}", game.black, game.event, game.datetime)
    } else {
      format!("[B] vs {} - {} @ {}", game.white, game.event, game.datetime)
    };
    game_selector.add_item(game_description, game);
  }
  game_selector.set_on_submit(move |s, game| {
    s.pop_layer();
    show_game(s, game, db_url.clone())
  });
  Dialog::around(ScrollView::new(game_selector).show_scrollbars(true).max_height(10)).title(format!("{games_number} games played by {player_name}"))
}

fn error_pop_up<T: std::fmt::Debug>(siv: &mut Cursive, err: T) {
  let debug = Dialog::around(TextView::new(format!("{:?}",err)));
  siv.add_layer(debug);
}

fn show_game(siv: &mut Cursive, game: &Game, db_url: String) {
  let board_state = fetch_game(db_url, game.id.id).expect("Could not find game");
  siv.add_layer(draw_board_state(&board_state));

  // TODO: rewrite this mess
  // possibly using View's internal `on_event`.
  let board_1 = Rc::new(RefCell::new(board_state));
  let board_2 = board_1.clone();
  siv.add_global_callback(Key::Right, move |s| {
    let mut board_state = board_1.borrow_mut();
    if board_state.curr_move_idx < board_state.moves.len() {
      board_state.curr_move_idx += 1;
      s.pop_layer();
      s.add_layer(draw_board_state(&board_state));
    }
  });
  siv.add_global_callback(Key::Left, move |s| {
    let mut board_state = board_2.borrow_mut();
    if board_state.curr_move_idx > 0 {
      board_state.curr_move_idx -= 1;
      s.pop_layer();
      s.add_layer(draw_board_state(&board_state))
    }
  });
}

pub fn draw_related_games_column(board_state: &BoardState) -> impl View {
  let empty = Vec::new();
  let related_board = board_state.related_games.get(board_state.curr_move_idx).unwrap_or(&empty);
  let mut lines = LinearLayout::vertical();
  for (played_move, game) in related_board.iter() {
    let mvmt = TextView::new(if played_move.game_round % 2 == 1 {
      format!("{} {}", (played_move.game_round + 1) / 2, played_move.san_plus.0)
    } else {
      format!("{} ... {}", played_move.game_round / 2, played_move.san_plus.0)
    });
    let game = TextView::new(format!("{} ({:?}) vs {} ({:?})", game.white, game.white_elo, game.black, game.black_elo));
    let layout = LinearLayout::horizontal().child(mvmt).child(DummyView).child(game);
    lines.add_child(layout);
  }
  let related_games = ScrollView::new(lines).show_scrollbars(true);
  Dialog::around(related_games).title(format!("{} games reach this position", related_board.len())).full_height()
    .max_width(44) // max width of board + movement column combined
    // TODO: figure out a way to calculate this stuff before hand
}
 
pub fn draw_movement_column(board_state: &BoardState) -> impl View {
  let mut white_column = LinearLayout::vertical();
  let mut black_column = LinearLayout::vertical();
  let mut mvmt_count_col = LinearLayout::vertical();
  let seen     = Style { effects: Effect::Dim | Effect::Strikethrough, color: Color::Dark(BaseColor::Black).into() };
  let not_seen = Color::Dark(BaseColor::Black).into();
  let current = Style { effects: Effect::Blink | Effect::Bold, color: Color::Light(BaseColor::Magenta).into() };
  for movement in board_state.moves.iter() {
    let order = (movement.game_round - 1).cmp(&(board_state.curr_move_idx as i32));
    let style = match order {
      std::cmp::Ordering::Less => seen,
      std::cmp::Ordering::Equal => current,
      std::cmp::Ordering::Greater => not_seen,
    };
    let mvmt_sans = TextView::new(format!("{}", movement.san_plus.0)).style(style);
    if movement.game_round % 2 == 1 {
      white_column.add_child(mvmt_sans);
      mvmt_count_col.add_child(TextView::new(format!("{}", (movement.game_round + 1)/2)))
    } else {
      black_column.add_child(mvmt_sans);
    }
  }
  let columns = LinearLayout::horizontal()
    .child(mvmt_count_col)
    .child(DummyView)
    .child(white_column)
    .child(DummyView)
    .child(black_column);
  ScrollView::new(columns).show_scrollbars(true).scroll_strategy(ScrollStrategy::KeepRow).max_height(9)
}

pub fn draw_chess_board(board_state: &BoardState) -> impl View {
  let mut board_column = LinearLayout::vertical();
  let chess_board = board_state.current_board();
  for row in (0..8).rev() {
    let mut row_layout = LinearLayout::horizontal()
      .child(DummyView)
      .child(TextView::new(format!("{}", row + 1)))
      .child(DummyView);
    for col in 0..8 {
      let square = Square::new(row * 8 + col);
      let piece = chess_board.piece_at(square);
      let cell = square_view(board_state, piece, square);
      row_layout.add_child(cell);
    }
    row_layout.add_child(DummyView);
    board_column.add_child(row_layout);
  }
  board_column.add_child(TextView::new("   A  B  C  D  E  F  G  H"));
  board_column
}

fn draw_board_state(board_state: &BoardState) -> impl View {
  let game_description = LinearLayout::vertical()
    .child(TextView::new(format!("{} [W] vs {} [B]", board_state.game.white, board_state.game.black)))
    .child(TextView::new(format!("{} {}", board_state.game.event, board_state.game.datetime)));
  let board = draw_chess_board(board_state);
  let movement_column = draw_movement_column(board_state);
  let middle = LinearLayout::horizontal().child(board).child(movement_column);
  let related_games = draw_related_games_column(board_state);
  let main_content = LinearLayout::vertical().child(Panel::new(game_description)).child(middle);
  LinearLayout::vertical().child(Panel::new(main_content)).child(related_games)
}
  
pub fn square_view(board_state: &BoardState, piece: Option<Piece>, square: Square) -> TextView {
  let piece_char = match piece {
    Some(Piece { role: Role::King, ..   }) => '\u{265A}',
    Some(Piece { role: Role::Queen, ..  }) => '\u{265B}',
    Some(Piece { role: Role::Rook, ..   }) => '\u{265C}',
    Some(Piece { role: Role::Bishop, .. }) => '\u{265D}',
    Some(Piece { role: Role::Knight, .. }) => '\u{265E}',
    Some(Piece { role: Role::Pawn, ..   }) => '\u{265F}',
    None => ' '
  };
  let piece_color = if let Some(Piece {color: PieceColor::White, ..}) = piece {
    Color::Dark(BaseColor::White)
  } else if let Some(Piece {color: PieceColor::Black, ..}) = piece {
    Color::Dark(BaseColor::Black)
  } else {
    Color::TerminalDefault
  };
  let (row, col) = {
    let index: u16 = square.into();
    (index / 8, index % 8)
  };
  let base_square_color = if row % 2 != col % 2 {
    Color::Light(BaseColor::Magenta)
  } else {
    Color::Dark(BaseColor::Magenta)
  };
  let to_color = Color::Light(BaseColor::Yellow);
  let from_color = Color::Dark(BaseColor::Yellow);
  // not my proudest code, but i think this works.
  let square_color = match (board_state.last_move_from_square(), board_state.last_move_to_square()) {
    (_,      Some(to)) if square == to   => to_color,
    (Some(from),    _) if square == from => from_color,
    _ => base_square_color
  };
  let style =ColorStyle::new(piece_color, square_color);
  TextView::new(format!(" {piece_char} ")).style(style)
}

