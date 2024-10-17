with import <nixpkgs> { };

mkShell {
  buildInputs = [
    pkg-config
    openssl
  ];
}
