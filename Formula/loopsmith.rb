# Homebrew formula for loopsmith.
#
# This copy is the source of truth. It is mirrored into the tap repository
# bitphill/homebrew-loopsmith at release time, which is what makes
#
#   brew tap bitphill/loopsmith && brew install loopsmith
#
# resolve by the bare name. A formula in homebrew-core would make the tap step
# unnecessary; that needs a review PR and a repository with more history behind
# it than this one has yet. Nothing below is tap-specific, so the same file can be
# submitted when that day comes.
#
# The `sha256` is the GitHub source tarball's, filled in at release time. Check the
# size before trusting the digest — codeload rate-limits unauthenticated archive
# downloads, `curl -sL` reports success while writing the error body, and hashing
# that yields a checksum no install can ever match:
#
#   curl -sL .../v0.1.3.tar.gz -o t.tar.gz && gzip -t t.tar.gz && wc -c < t.tar.gz
#   shasum -a 256 t.tar.gz        # only once the file is ~2 MB of valid gzip
#
# `gh api repos/.../tarball/<ref>` is not a substitute: it mangles binary output
# and returns a 199-byte fragment for every ref.
class Loopsmith < Formula
  desc "Self-evolving agent loops behind a deterministic verification gate"
  homepage "https://github.com/bitphill/loopsmith"
  url "https://github.com/bitphill/loopsmith/archive/refs/tags/v0.2.2.tar.gz"
  sha256 "c70ada12c55d2834e2146e8d33c0c3e2661cd230d4c0a8ea1ceb0872ff4e1976"
  license "MIT"
  head "https://github.com/bitphill/loopsmith.git", branch: "main"

  depends_on "rust" => :build

  def install
    # The cargo workspace root is `runtime/`, not the repository root, and the
    # binary comes from the `loopsmith` package inside it.
    cd "runtime" do
      system "cargo", "install", *std_cargo_args(path: "crates/loopsmith-cli")
    end
  end

  test do
    assert_match "loopsmith #{version}", shell_output("#{bin}/loopsmith --version")

    # `doctor` is the honest smoke test: it probes the host and must stay
    # advisory, so a constrained CI container cannot make it fail.
    doctor = shell_output("#{bin}/loopsmith doctor")
    assert_match "platform", doctor
    assert_match "userland", doctor

    # A scaffolded loop must refuse to validate until its `pre_execution` steps
    # are marked done. That refusal is the product, so a build where it stops
    # happening is a broken build.
    system bin/"loopsmith", "new", "--path", testpath/"demo", "--purpose", "brew test"
    assert_path_exists testpath/"demo/loop.yaml"
    assert_path_exists testpath/"demo/run.sh"
    assert_path_exists testpath/"demo/run.cmd"

    output = shell_output("#{bin}/loopsmith validate #{testpath}/demo/loop.yaml 2>&1", 1)
    assert_match "pre_execution", output
  end
end
