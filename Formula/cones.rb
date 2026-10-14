class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.16.tar.gz"
  sha256 "48aeaab214ee31646fdc08293bf6db1ff5fe01ea7a6020b8117a5a64da4fc2ac"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.16"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "08f1248e095d14351e9aec0cbc22acdb9cf28ee8f0f064081dab80c285dbea1a"
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
