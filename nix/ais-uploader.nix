{
  fetchFromGitHub,
  rustPlatform,
  pkg-config,
  openssl,
  lib,
}:
rustPlatform.buildRustPackage (
  finalAttrs: {
    pname = "ais_forwarder";
    version = "0.1.2";

    nativeBuildInputs = [pkg-config];
    buildInputs = [openssl];

    src = ./..;

    cargoHash = "sha256-8JZk4j5yvzmBQK/02KbcqNY/lTSr+L6jefzUGCTcW1M=";
    # cargoHash = lib.fakeHash;
    strip = true;

    meta = {
      description = "an AIS upload and forwarding tool";
      homepage = "https://github.com/orbitsailor/ais-uploader";
      license = lib.licenses.mit;
    };
  }
)
