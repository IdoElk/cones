class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.20.tar.gz"
  sha256 "b7b44868f3b5bcd1d6b0f9a53c3e50c660d36ddfb382a37a41bd8268db8c4646"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.20"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "a8a71034b820966835687766529b163d8a43a72f39e65e015322a77b569f4051"
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
