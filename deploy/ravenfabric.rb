class Ravenfabric < Formula
  desc "Secure remote execution and mesh networking agent — zero-trust, cryptographically verified"
  homepage "https://ravenfabric.io"
  version "0.1.4"
  license "AGPL-3.0-or-later"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/egkristi/RavenFabric-Published/releases/download/v0.1.4/ravenfabric-darwin-arm64-cli"
      sha256 "faf2615ace5237b9729621e4af9fb0b28a983bde673068efa01ab7fbba455353"
    else
      url "https://github.com/egkristi/RavenFabric-Published/releases/download/v0.1.4/ravenfabric-darwin-amd64-cli"
      sha256 "06db906170107aa1e619b7777ebcccdcd2b6b0f197cd24764d80bdf0e881c5a5"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/egkristi/RavenFabric-Published/releases/download/v0.1.4/ravenfabric-linux-arm64-musl-cli"
      sha256 "4007bba6bde405b08a4eda3f60a89678e1cbf4e6ac66fa257127ddf50ddbb88f"
    elsif Hardware::CPU.intel?
      url "https://github.com/egkristi/RavenFabric-Published/releases/download/v0.1.4/ravenfabric-linux-amd64-musl-cli"
      sha256 "6eb1388765f10616b706187c285a1bc9b361611a939fb3e868f9f22dd2ba2ed7"
    end
  end

  def install
    bin.install Dir.glob("ravenfabric-*").first => "rf"
  end

  test do
    assert_match "rf", shell_output("#{bin}/rf --help")
  end
end
