# typed: false
# frozen_string_literal: true

class Ravenfabric < Formula
  desc "Secure remote execution and mesh networking agent — zero-trust, cryptographically verified"
  homepage "https://ravenfabric.io"
  url "https://github.com/egkristi/RavenFabric/archive/refs/tags/v0.1.4.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "AGPL-3.0-or-later"
  head "https://github.com/egkristi/RavenFabric.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "-p", "rf-agent", "-p", "rf-relay", "-p", "rf-cli"
    bin.install "target/release/rf-agent"
    bin.install "target/release/rf-relay"
    bin.install "target/release/rf" => "rf"

    # Generate shell completions
    output = Utils.safe_popen_read(bin/"rf", "completions", "bash")
    (bash_completion/"rf").write output
    output = Utils.safe_popen_read(bin/"rf", "completions", "zsh")
    (zsh_completion/"_rf").write output
    output = Utils.safe_popen_read(bin/"rf", "completions", "fish")
    (fish_completion/"rf.fish").write output
  end

  def post_install
    (var/"log/ravenfabric").mkpath
    (etc/"ravenfabric").mkpath
  end

  service do
    run [opt_bin/"rf-agent", "--config", etc/"ravenfabric/raven.toml"]
    keep_alive true
    log_path var/"log/ravenfabric/agent.log"
    error_log_path var/"log/ravenfabric/agent.err"
    working_dir var/"lib/ravenfabric"
    environment_variables RUST_LOG: "info"
  end

  test do
    assert_match "rf-agent", shell_output("#{bin}/rf-agent --help")
    assert_match "rf", shell_output("#{bin}/rf --help")
  end
end
