class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.10.tar.gz"
  sha256 "f33754459c14efef9064e4fd6cbdc93656fc876b47f57a7d1e5300fe28e938ef"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.10"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "2d90bee2c93f9e03c78eabad837bd532414b2b7d86a61df5d6e66edb0177fb15"
  end

  depends_on "rust" => :build
  depends_on :macos

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "cones #{version}", shell_output("#{bin}/cones --version")
  end
end
