{ pkgs, inputs, ... }:
{
  packages = [
    inputs.claude-code-nix.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
  };
}
