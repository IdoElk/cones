class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.15.tar.gz"
  sha256 "f93c88fb19b1af00f75b4300d95b815eb7cd3d11e550d84dd3f2bf5b60cf72c3"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.15"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "f8534f68321e4f84b69289754dd62244f93248f9800032b49d0096f81e2b10a5"
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
