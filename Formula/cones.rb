class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.14.tar.gz"
  sha256 "9d944f8d80be59fc07cc93ca26a17641bcf78d2c8c8bfec84f3d3e311506ad9a"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.14"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "efa09fb92de9f4de6d4daf7c94320f099d3bb903cee217d3734703902a23c9e2"
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
