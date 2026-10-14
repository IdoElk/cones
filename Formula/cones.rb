class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.18.tar.gz"
  sha256 "7686a18d12a839ee654f19992d480aa29c3e281073a20f9d945e55c46ec07ecd"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.18"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "ecfa8ad064540a5c13c850622930ec4ae63481139cd62f57d3cec69efd937b0a"
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
