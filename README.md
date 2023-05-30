# Nixchess

A terminal based chess opening explorer.

![Screenshot of chessboard in terminal](./screenshot.png "Screenshot of chess board in terminal")

*using the Fixedsys font on Windows Terminal*

## Buiding
This project can be built from withing a pure nix shell. To build it:
```sh
cd ~/path/to/this/repository
nix-shell --pure
cargo install --path=.
```

## Filling the games
A valid database be built from a raw pgn file, like any file from the [open lichess games database](https://database.lichess.org/). First, create an empty instance:
```sh
# If you're not already in the nix-shell
nix-shell
# Download and decompress a small sample file
curl https://database.lichess.org/standard/lichess_db_standard_rated_2014-01.pgn.zst | zstd -d -o lichess_db_standard_rated_2014-01.pgn
# Create a sandbox database
scm sandbox -n pkg/nixchess-*
```
This will create a postgres server in `~/var/pg` with the correct schema, already running. Then, you must find the port that the server is listening (for example, running `scm status`), and run the following command:
```
nixchess fill [PGN_FILE_PATH] --db_url postgres://root:root@localhost:{DB_INSTANCE_PORT}/sandbox
```
You can set the `DATABASE_URL` environment variable in the `.env` file (or in your current session), so that you do not need to repeat this flag everytime.

If a database instance already exists, but is offline, then you can start it using
```sh
scm start ~/var/pg/sandbox-*
```
