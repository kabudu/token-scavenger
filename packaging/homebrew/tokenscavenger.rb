class Tokenscavenger < Formula
  desc "Self-hosted OpenAI-compatible LLM proxy/router"
  homepage "https://github.com/kabudu/token-scavenger"
  version "0.3.4"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/kabudu/token-scavenger/releases/download/v#{version}/tokenscavenger-v#{version}-aarch64-apple-darwin.zip"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/kabudu/token-scavenger/releases/download/v#{version}/tokenscavenger-v#{version}-x86_64-unknown-linux-gnu"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    end
  end

  def install
    candidate = Dir["tokenscavenger*"].find { |path| File.file?(path) }
    bin.install candidate => "tokenscavenger"
  end

  service do
    run [opt_bin/"tokenscavenger", "--config", "#{etc}/tokenscavenger/tokenscavenger.toml"]
    keep_alive true
    working_dir var
    log_path var/"log/tokenscavenger.log"
    error_log_path var/"log/tokenscavenger.err.log"
  end

  test do
    assert_match "tokenscavenger", shell_output("#{bin}/tokenscavenger --help")
  end
end
