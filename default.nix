let
  scm_repo = (fetchTarball {
    url = let commit = "b23f0f3c9b985b74599d553b0a8b1c453309b0ed"; in "https://gitlab.com/deltaex/schematic/-/archive/${commit}/schematic-${commit}.tar.gz";
    sha256 = "sha256:1a0rcdhn9ymrn3zkmr6b90cipw34hrlxqmlw8ipilwsnzph7pyjh";
  });
  scm = (import scm_repo {
    verbose = true;
    repos = [
      "."
      scm_repo
    ];
  });
  mozilla = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  nixpkgs = import <nixpkgs> { overlays = [ mozilla ]; };
in

with nixpkgs;

{
  schematic = scm.shell.overrideAttrs(new: old: {
    buildInputs = old.buildInputs ++ [ncurses openssl.dev cargo rustc];
    nativeBuildInputs = old.buildInputs ++ [pkg-config];
    packages = [cargo];
  });
}

