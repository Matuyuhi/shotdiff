# Template for Matuyuhi/homebrew-tools/Formula/shotdiff.rb.
# Values are substituted and pushed by Matuyuhi/shotdiff (.github/workflows/release.yml) on each release.

class Shotdiff < Formula
  desc "Side-by-side screenshot diff: BEFORE | DIFF | AFTER, changes in pink"
  homepage "https://github.com/Matuyuhi/shotdiff"
  version "__VERSION__"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/Matuyuhi/shotdiff/releases/download/v#{version}/shotdiff-aarch64-apple-darwin.tar.gz"
      sha256 "__SHA_MACOS_ARM__"
    else
      url "https://github.com/Matuyuhi/shotdiff/releases/download/v#{version}/shotdiff-x86_64-apple-darwin.tar.gz"
      sha256 "__SHA_MACOS_X86__"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/Matuyuhi/shotdiff/releases/download/v#{version}/shotdiff-aarch64-linux.tar.gz"
      sha256 "__SHA_LINUX_ARM__"
    else
      url "https://github.com/Matuyuhi/shotdiff/releases/download/v#{version}/shotdiff-x86_64-linux.tar.gz"
      sha256 "__SHA_LINUX_X86__"
    end
  end

  def install
    bin.install "shotdiff"
  end

  test do
    assert_match "shotdiff", shell_output("#{bin}/shotdiff --help")
  end
end
