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
  oxalica = [ (import (builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz")) ];
  nixpkgs = import <nixpkgs> { overlays = oxalica; };
in

with nixpkgs;

{
  schematic = scm.shell.overrideAttrs(new: old: {
    buildInputs = old.buildInputs ++ [openssl.dev rust-bin.stable."1.69.0".minimal nix];
    nativeBuildInputs = old.buildInputs ++ [pkg-config];
  });
}
