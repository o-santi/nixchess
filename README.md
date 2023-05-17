# Nixchess

A chess opening explorer.

![Screenshot of chessboard in terminal](./screenshot.png "Screenshot of chess board in terminal")

*using the Fixedsys font on Windows Terminal*

## Running
To run
```rs
cargo run -- view --db_url {DATABASE_INSTANCE_URL}
```
It requires a valid chess game database running. It can be built from a raw pgn file, like any file from the [open lichess games database](https://database.lichess.org/). First, create an empty database instance:
```sh
nix-shell
scm sandbox -n pkg/nixchess-*
```
This will create a postgres instance in `~/var/pg` with the correct schema. Then, you must find the port that the server is listening (for example, running `scm status`), and run the following command:
```
cargo run -- fill [PGN_FILE_PATH] --db_url postgres://root:root@localhost:{DB_INSTANCE_PORT}/sandbox
```
You can set the `DATABASE_URL` environment variable in the `.env` file (or in your current session), so that you do not need to repeat this flag everytime.

If a database instance already exists, but is offline, then you can start it using
```sh
scm start ~/var/pg/sandbox-*
```
