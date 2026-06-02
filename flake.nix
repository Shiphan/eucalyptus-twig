{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell rec {
        buildInputs = with pkgs; [
          fenix.packages.${system}.default.rustfmt
          clippy

          libxkbcommon.dev
          fontconfig.dev
          wayland
          vulkan-loader
          pkg-config
          dbus.dev
          pipewire.dev
        ];
        LIBCLANG_PATH = nixpkgs.lib.makeLibraryPath (with pkgs; [ libclang.lib ]);
        LD_LIBRARY_PATH = nixpkgs.lib.makeLibraryPath buildInputs;
      };
      formatter.${system} = pkgs.nixfmt-tree;
    };
}
