use log::{LevelFilter, warn};
use sqlx::{Connection, PgConnection};
use nixchess::{ui::cli_entrypoint, db::insert_games_from_file};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
/// A chess game visualizer and database builder.
struct NixChessArgs {
  #[clap(subcommand)]
  command: Command,
  /// Games database to connect to. If none, uses the `DATABASE_URL` environment variable.
  #[clap(short, long)]
  db_url: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// Run chess game vizualizer 
  View,
  /// Fill the database from a pgn file
  Fill {
    pgn_file: String
  }
}

fn main() {
  let args = NixChessArgs::parse();
  simple_logging::log_to_file("view.log", LevelFilter::Warn).expect("Could not start logger");

  std::panic::set_hook(Box::new(|err| {
    warn!("{}", err);
  }));
  
  let db_url = args.db_url.unwrap_or_else(|| {
    dotenv::dotenv().ok();
    std::env::var("DATABASE_URL").expect("No database url in .env. Please provide one using -db url.")
  });

  
  match args.command {
    Command::View => cli_entrypoint(db_url),
    Command::Fill { pgn_file } => {
      let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
      runtime.block_on(async {
        let mut pool = PgConnection::connect(&db_url).await.expect("Could not connect to the database");
        insert_games_from_file(&mut pool, &pgn_file).await.unwrap();
      })
    },
  };
}
