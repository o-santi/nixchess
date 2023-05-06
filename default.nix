with builtins;
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

  # rustOverlay = builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz";  
  # pkgs = import pinnedPkgs {
  #   overlays = [ (import rustOverlay)];
  # };
in rec {
  schematic = scm.shell;
}    
