{ pkgs, lib, config, inputs, ... }:

{
  # https://devenv.sh/packages/
  packages = with pkgs; [ git libyaml openssl zig ];

  languages.rust.enable = true;


  enterShell = ''

  '';

}
