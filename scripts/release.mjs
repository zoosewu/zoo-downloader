// Prepare a release: set the version, commit, tag. Pushing the tag starts the release
// workflow (.github/workflows/release.yml), which builds every platform and publishes; the
// build writes the tag's version into the project itself, so the commit only keeps main in step.
//   pnpm release 0.2.0        or: pnpm release patch | minor | major
// Nothing is pushed; the script prints the command.
import { execFileSync } from 'node:child_process';
import { setVersion, versions } from './version.mjs';

const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();

function next(current, bump) {
  const [major, minor, patch] = current.split(/[.-]/).map(Number);
  switch (bump) {
    case 'major':
      return `${major + 1}.0.0`;
    case 'minor':
      return `${major}.${minor + 1}.0`;
    case 'patch':
      return `${major}.${minor}.${patch + 1}`;
    default:
      return bump;
  }
}

const arg = process.argv[2];
if (!arg) {
  console.error('usage: pnpm release <X.Y.Z | patch | minor | major>');
  process.exit(2);
}
if (git('status', '--porcelain')) {
  console.error('commit or stash your changes first');
  process.exit(1);
}
if (git('branch', '--show-current') !== 'main') {
  console.error('release from main');
  process.exit(1);
}

const current = versions()['package.json'];
const version = next(current, arg);
const tag = `v${version}`;
if (git('tag', '--list', tag)) {
  console.error(`${tag} already exists`);
  process.exit(1);
}

setVersion(version);
git('add', 'package.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock');
// chore(release) is left out of the release notes (zoo-release's git-cliff settings).
git('commit', '-m', `chore(release): ${tag}`);
git('tag', '-a', tag, '-m', tag);
console.log(`${current} -> ${version}; committed and tagged ${tag}.`);
console.log(`Publish with:  git push origin main ${tag}`);
