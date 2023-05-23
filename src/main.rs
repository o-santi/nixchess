use log::warn;
use sqlx::postgres::PgPoolOptions;
use nixchess::{ui::cli_entrypoint, db::{insert_games_from_file, InsertionError}};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
/// A chess game visualizer and database builder.
struct NixChessArgs {
  #[clap(subcommand)]
  command: Option<Command>,
  /// Games database to connect to. If none, uses the `DATABASE_URL` environment variable.
  #[clap(short, long)]
  db_url: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// Fill the database from a pgn file
  Fill {
    pgn_file: String
  }
}

fn main() -> Result<(), InsertionError> {
  let args = NixChessArgs::parse();
  
  // simple_logging::log_to_file("view.log", LevelFilter::Warn).expect("Could not start logger");
  let db_url = args.db_url.unwrap_or_else(|| {
    dotenv::dotenv().ok();
    std::env::var("DATABASE_URL").expect("No database url in .env. Please provide one using -db url.")
  });
  match args.command {
    None => {
      std::panic::set_hook(Box::new(|err| {
        warn!("{err}");
      }));
      cursive::logger::init(); // enables debugging console.
      
      cli_entrypoint(db_url);
      
      Ok(())
    },
    Some(Command::Fill { pgn_file }) => {
      let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .build()?;
      runtime.block_on(async {
        let pool = PgPoolOptions::new()
          .min_connections(50)
          .max_connections(100)
          .connect(&db_url).await?;
        insert_games_from_file(pool, &pgn_file).await?;
        Ok::<(), InsertionError>(())
      })
    },
  }
}
