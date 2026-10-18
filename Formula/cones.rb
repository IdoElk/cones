class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.4.0.tar.gz"
  sha256 "da3cb2347ea69b79a7fc480d6436d80cfac3111b916b0d2175f7b2ec9655a04a"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.4.0"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "b4281323477b190d8f615e222767b30b741a45d4ebd2930bdd8bdf753b47db38"
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
