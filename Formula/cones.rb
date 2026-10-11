class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.11.tar.gz"
  sha256 "12531c7161da669a03f179e83171f22c62be5540e040d4c511d912d29524aa2c"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.11"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "9c3ef47e9cab06b060a533bb63afb74ad07cba316e263b511828a66a67170bf1"
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
