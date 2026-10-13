class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.12.tar.gz"
  sha256 "216ea83e560a830dcc36346407398a55c918b62fdc9af9461f76a32c92fb1a04"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.12"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "d01f738754abf3f06cae4ee155872910230efd979529f1089ad44ca0d1e1682a"
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
