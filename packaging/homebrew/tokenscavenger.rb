class Tokenscavenger < Formula
  desc "Self-hosted OpenAI-compatible LLM proxy/router"
  homepage "https://github.com/kabudu/token-scavenger"

  on_macos do
    on_arm do
      url "https://github.com/kabudu/token-scavenger/releases/download/v0.3.5/tokenscavenger-v0.3.5-aarch64-apple-darwin.zip"
      sha256 "83cea09216068ccae7a7748730e1629a9a94d5c02c24b4b051ffcc155af490fe"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/kabudu/token-scavenger/releases/download/v0.3.5/tokenscavenger-v0.3.5-x86_64-unknown-linux-gnu"
      sha256 "e61cb4420032baf900d7d515e8d1241759f8750e0a645ce88d083aca736a2638"
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
