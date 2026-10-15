class Cones < Formula
  desc "A terminal workspace for coding agents"
  homepage "https://github.com/YuvalSarel1/cones"
  url "https://github.com/YuvalSarel1/cones/archive/refs/tags/v0.3.19.tar.gz"
  sha256 "35d49e054be616c91c0492fc67cddde32d7bdb2fd572d99cc5954fb30465f136"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/YuvalSarel1/cones.git", branch: "main"

  bottle do
    root_url "https://github.com/YuvalSarel1/cones/releases/download/v0.3.19"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "14fd6d6a540da834665b688c9382a101f4443653b8ade9b7b16090322ea3a6eb"
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
