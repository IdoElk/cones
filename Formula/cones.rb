class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.21.tar.gz"
  sha256 "3b0d51e927404d4316d098074a5f981bec0ff918c9e0a33103145bf620105bf1"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.21"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "93ceedce80817dad549925eb21f922bf389e233bea16080bbc721039abe53330"
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
