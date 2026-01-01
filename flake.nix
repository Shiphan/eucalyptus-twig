{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell rec {
        buildInputs = with pkgs; [
          libxkbcommon.dev
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
