class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.17.tar.gz"
  sha256 "985ba809dd2227edc55492c866f74c6bd62a9850bf55aa2e8f44824679d1466c"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.17"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "8a141b1e8ea499f675ac6573b75122997902f26fa87dac900a6c439b7e852f05"
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
