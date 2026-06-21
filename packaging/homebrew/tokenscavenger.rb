class Tokenscavenger < Formula
  desc "Self-hosted OpenAI-compatible LLM proxy/router"
  homepage "https://github.com/kabudu/token-scavenger"

  on_macos do
    on_arm do
      url "https://github.com/kabudu/token-scavenger/releases/download/v0.3.4/tokenscavenger-v0.3.4-aarch64-apple-darwin.zip"
      sha256 "438bca5d8cf9a4a97d2c98c566c79319dbb1a3082b7800c72ae3e57e77cd0435"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/kabudu/token-scavenger/releases/download/v0.3.4/tokenscavenger-v0.3.4-x86_64-unknown-linux-gnu"
      sha256 "c0cb6dbd7347ab9cafd3e42f4b5684b6a06953d9918439361e88644c55a7d459"
    end
  end

  def install
    candidate = Dir["tokenscavenger*"].find { |path| File.file?(path) }
    odie "tokenscavenger release binary was not found" unless candidate

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
