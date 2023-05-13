* Nixchess (Name TDB)

A chess game terminal visualizer.

To run
```rs
cargo run -- view
```

This requires a valid chess game database. It can be built from a pgn file, in the following way:
```sh
nix-shell
scm sandbox -n pkg/nixchess-*
```
This will create a postgres database instance in `~/var/pg`. Then, all you must do is find the port that the server is listening, and run the following command:
```
cargo run -- fill [PGN_FILE_PATH] --db_url postgres://root:root@localhost:[DB_URL_PORT]/sandbox
```

