CREATE TABLE Player (
  player_name text PRIMARY KEY
);

CREATE TABLE Game (
  id SERIAL PRIMARY KEY,
  event TEXT NOT NULL,
  datetime TIMESTAMP NOT NULL,
  white TEXT REFERENCES Player(player_name) NOT NULL,
  black TEXT REFERENCES Player(player_name) NOT NULL,
  white_elo INTEGER,
  black_elo INTEGER
);

CREATE TABLE Move (
  game_round INTEGER NOT NULL,
  game_id INTEGER REFERENCES Game(id) NOT NULL,
  san_plus TEXT NOT NULL,
  board_hash BIGINT NOT NULL -- 8 bytes
);
