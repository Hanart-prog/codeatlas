//! Deterministic fixture repositories.
//!
//! Commits are built directly in the object database with fixed identities and
//! timestamps, so the same script always produces the same object ids, and the
//! worktree stays empty, which proves that analysis reads Git objects only.
//!
//! This crate is the only one in the workspace that writes to a repository, and
//! it is used by tests and the demo, never by the engine.

use std::collections::BTreeMap;
use std::path::Path;

use git2::build::TreeUpdateBuilder;
use git2::{FileMode, Repository, RepositoryInitOptions, Signature, Time};

pub use git2::Oid;

/// First fixture timestamp: 2024-01-01T00:00:00Z.
const EPOCH: i64 = 1_704_067_200;
const HOUR: i64 = 3600;

/// `count` lines of TypeScript whose names start with `name`.
pub fn exports(name: &str, count: usize) -> String {
    (0..count)
        .map(|i| format!("export const {name}{i} = {i};\n"))
        .collect()
}

/// Builds commits on a repository.
pub struct FixtureRepo {
    repository: Repository,
    head: Option<Oid>,
    clock: i64,
}

impl FixtureRepo {
    /// Creates a repository in `directory` whose default branch is `main`.
    pub fn init(directory: &Path) -> Result<Self, git2::Error> {
        let mut options = RepositoryInitOptions::new();
        options.initial_head("main");
        Ok(Self {
            repository: Repository::init_opts(directory, &options)?,
            head: None,
            clock: EPOCH,
        })
    }

    /// The latest commit on the current line.
    pub fn head(&self) -> Option<Oid> {
        self.head
    }

    /// Moves the current line to `oid` without touching `main`.
    pub fn branch_from(&mut self, oid: Oid) {
        self.head = Some(oid);
    }

    /// Sets the clock for the next commit, for out-of-order timestamps.
    pub fn set_clock(&mut self, seconds: i64) {
        self.clock = seconds;
    }

    /// Makes `HEAD` point directly at `oid`.
    pub fn detach_head(&self, oid: Oid) -> Result<(), git2::Error> {
        self.repository.set_head_detached(oid)
    }

    /// The underlying repository.
    pub fn repository(&self) -> &Repository {
        &self.repository
    }

    /// Starts a commit on the current line.
    pub fn change(&mut self) -> Result<Change<'_>, git2::Error> {
        let baseline = match self.head {
            Some(oid) => self.repository.find_commit(oid)?.tree()?.id(),
            None => self.repository.treebuilder(None)?.write()?,
        };
        Ok(Change {
            fixture: self,
            baseline,
            upserts: BTreeMap::new(),
            removals: Vec::new(),
            second_parent: None,
            detached: false,
        })
    }

    fn signature(&self) -> Result<Signature<'static>, git2::Error> {
        Signature::new(
            "Fixture Author",
            "fixture@example.invalid",
            &Time::new(self.clock, 0),
        )
    }
}

/// A commit being assembled. Paths are bytes so that fixtures can include paths
/// that are not valid UTF-8.
pub struct Change<'a> {
    fixture: &'a mut FixtureRepo,
    baseline: Oid,
    upserts: BTreeMap<Vec<u8>, (Oid, FileMode)>,
    removals: Vec<Vec<u8>>,
    second_parent: Option<Oid>,
    detached: bool,
}

impl Change<'_> {
    /// Writes a regular file.
    pub fn write(self, path: impl AsRef<[u8]>, content: impl AsRef<[u8]>) -> Self {
        self.entry(path, content, FileMode::Blob)
    }

    /// Writes a symbolic link.
    pub fn symlink(self, path: impl AsRef<[u8]>, target: &str) -> Self {
        self.entry(path, target, FileMode::Link)
    }

    /// Writes a submodule pointer to `commit`.
    pub fn submodule(mut self, path: impl AsRef<[u8]>, commit: Oid) -> Self {
        self.upserts
            .insert(path.as_ref().to_vec(), (commit, FileMode::Commit));
        self
    }

    /// Appends to a file that exists in this commit's parent.
    pub fn append(self, path: &str, content: &str) -> Self {
        let mut existing = self.read(path);
        existing.extend_from_slice(content.as_bytes());
        self.write(path, existing)
    }

    /// Replaces text in a file that exists in this commit's parent.
    pub fn replace(self, path: &str, from: &str, to: &str) -> Self {
        let existing = String::from_utf8(self.read(path)).expect("fixture files are UTF-8");
        assert!(existing.contains(from), "{path} does not contain {from:?}");
        self.write(path, existing.replace(from, to))
    }

    /// Moves a file without changing its content.
    pub fn rename(self, from: &str, to: &str) -> Self {
        let content = self.read(from);
        self.remove(from).write(to, content)
    }

    /// Deletes a file.
    pub fn remove(mut self, path: impl AsRef<[u8]>) -> Self {
        self.removals.push(path.as_ref().to_vec());
        self
    }

    /// Adds a second parent, making this a merge commit.
    pub fn merge_of(mut self, parent: Oid) -> Self {
        self.second_parent = Some(parent);
        self
    }

    /// Commits without moving `HEAD`, for side branches.
    pub fn detached(mut self) -> Self {
        self.detached = true;
        self
    }

    /// Creates the commit and advances the clock by an hour.
    pub fn commit(self, message: &str) -> Result<Oid, git2::Error> {
        let repository = &self.fixture.repository;
        let baseline = repository.find_tree(self.baseline)?;
        let mut update = TreeUpdateBuilder::new();
        for path in &self.removals {
            update.remove(path.as_slice());
        }
        for (path, (oid, mode)) in &self.upserts {
            update.upsert(path.as_slice(), *oid, *mode);
        }
        let tree = repository.find_tree(update.create_updated(repository, &baseline)?)?;

        let parents = [self.fixture.head, self.second_parent]
            .into_iter()
            .flatten()
            .map(|oid| repository.find_commit(oid))
            .collect::<Result<Vec<_>, _>>()?;
        let parents: Vec<_> = parents.iter().collect();

        let signature = self.fixture.signature()?;
        let reference = (!self.detached).then_some("HEAD");
        let oid = repository.commit(reference, &signature, &signature, message, &tree, &parents)?;
        self.fixture.head = Some(oid);
        self.fixture.clock += HOUR;
        Ok(oid)
    }

    fn entry(mut self, path: impl AsRef<[u8]>, content: impl AsRef<[u8]>, mode: FileMode) -> Self {
        let oid = self
            .fixture
            .repository
            .blob(content.as_ref())
            .expect("the object database accepts blobs");
        self.upserts.insert(path.as_ref().to_vec(), (oid, mode));
        self
    }

    fn read(&self, path: &str) -> Vec<u8> {
        let repository = &self.fixture.repository;
        let tree = repository
            .find_tree(self.baseline)
            .expect("baseline exists");
        let entry = tree
            .get_path(Path::new(path))
            .unwrap_or_else(|_| panic!("{path} is not in the parent commit"));
        repository
            .find_blob(entry.id())
            .expect("entry is a blob")
            .content()
            .to_vec()
    }
}

/// The history `fixtures/expected/main.json` describes. Each commit is annotated
/// with what it does, so the expected numbers can be checked by hand.
pub fn generate_main_fixture(directory: &Path) -> Result<Oid, git2::Error> {
    const SESSION: &str = "src/auth/session.ts";
    const TOKEN: &str = "src/auth/token.ts";
    const ROUTES: &str = "src/api/routes.ts";
    const PROFILE: &str = "src/users/profile.ts";
    const ACCOUNT: &str = "src/users/account.ts";
    const INVOICE: &str = "src/billing/invoice.ts";
    const AUTH_TEST: &str = "tests/auth.test.ts";

    let mut repo = FixtureRepo::init(directory)?;

    // 0: root commit. session 10, token 6, routes 8, profile 5, README 3, a binary.
    let session = format!(
        "import {{ token0 }} from './token';\n{}",
        exports("session", 9)
    );
    let routes = format!(
        "import {{ session0 }} from '../auth/session';\n{}",
        exports("route", 7)
    );
    repo.change()?
        .write(SESSION, session)
        .write(TOKEN, exports("token", 6))
        .write(ROUTES, routes)
        .write(PROFILE, exports("profile", 5))
        .write("README.md", "# Fixture\n\nGenerated for tests.\n")
        .write("assets/logo.png", b"\x89PNG\r\n\x1a\n\0\0\0binary")
        .commit("Create auth, API and users modules")?;

    // 1: billing and its tests. invoice 7, auth test 4.
    let invoice = format!(
        "import {{ session0 }} from '../auth/session';\n{}",
        exports("invoice", 6)
    );
    let auth_test = format!(
        "import {{ session0 }} from '../src/auth/session';\n{}",
        exports("test", 3)
    );
    repo.change()?
        .write(INVOICE, invoice)
        .write(AUTH_TEST, auth_test)
        .commit("Add billing")?;

    // 2-4: auth and billing change together.
    repo.change()?
        .append(SESSION, &exports("refresh", 2))
        .append(INVOICE, &exports("seat", 1))
        .commit("Bill by session seat")?;
    repo.change()?
        .append(SESSION, &exports("expiry", 1))
        .append(INVOICE, &exports("proration", 2))
        .commit("Prorate expired sessions")?;
    repo.change()?
        .append(SESSION, &exports("device", 3))
        .append(INVOICE, &exports("device", 1))
        .commit("Track device sessions on invoices")?;

    // 5: a pure rename.
    repo.change()?
        .rename(PROFILE, ACCOUNT)
        .commit("Rename profile to account")?;

    // 6: edits to the renamed file. account +2, routes +1.
    repo.change()?
        .append(ACCOUNT, &exports("email", 2))
        .append(ROUTES, &exports("accountRoute", 1))
        .commit("Add account routes")?;

    // 7: adds two imports. Token now imports session, which already imports token,
    // so this introduces a cycle. routes +1, token +1.
    let side_base = repo
        .change()?
        .append(ROUTES, "import { invoice0 } from '../billing/invoice';\n")
        .append(TOKEN, "import { session0 } from './session';\n")
        .commit("Introduce session and token cycle")?;

    // Side branch, off the first-parent chain: tax 6, then invoice +2.
    let side_one = repo
        .change()?
        .detached()
        .write("src/billing/tax.ts", exports("tax", 6))
        .commit("Add tax module")?;
    repo.branch_from(side_one);
    let side_two = repo
        .change()?
        .detached()
        .append(INVOICE, &exports("tax", 2))
        .commit("Apply tax to invoices")?;

    // 8: merges the side branch. Against its first parent: tax 6, invoice +2.
    repo.branch_from(side_base);
    repo.change()?
        .write("src/billing/tax.ts", exports("tax", 6))
        .append(INVOICE, &exports("tax", 2))
        .merge_of(side_two)
        .commit("Merge branch 'tax'")?;

    // 9: deletes the auth test, removing 4 lines.
    repo.change()?
        .remove(AUTH_TEST)
        .commit("Remove obsolete auth test")?;

    // 10: removes the import from token, which resolves the cycle. token -1.
    repo.change()?
        .replace(TOKEN, "import { session0 } from './session';\n", "")
        .commit("Break session and token cycle")?;

    // 11: a new file at the deleted path is a new identity. 3 lines.
    repo.change()?
        .write(AUTH_TEST, exports("rewritten", 3))
        .commit("Rewrite auth test")?;

    // 12: one eligible file with a Unicode path, and four that metrics must skip:
    // two rule matches, a symlink, and a path that is not valid UTF-8.
    repo.change()?
        .write("src/módulo uno/índice.ts", exports("modulo", 2))
        .write("node_modules/pkg/index.js", "module.exports = 1;\n")
        .write("dist/app.js", "console.log(1);\n")
        .symlink("latest", "README.md")
        .write(b"src/caf\xe9.ts".as_slice(), exports("cafe", 1))
        .commit("Add vendored output and odd paths")?;

    Ok(repo.head.expect("the fixture has commits"))
}

/// A small deterministic generator, so that fixtures need no dependency.
struct Sequence(u64);

impl Sequence {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((self.0 >> 33) % bound as u64) as usize
    }
}

/// Modules of the demo repository, each with files that tend to change together.
const DEMO_MODULES: [(&str, [&str; 4]); 6] = [
    ("auth", ["session", "token", "password", "policy"]),
    ("billing", ["invoice", "plan", "tax", "refund"]),
    ("users", ["account", "profile", "invite", "roles"]),
    ("api", ["routes", "errors", "limits", "schema"]),
    ("search", ["index", "query", "ranking", "filters"]),
    ("notify", ["email", "digest", "webhook", "templates"]),
];

/// An invented product's history, big enough for every dashboard page to have
/// something to show. Most commits change files of one module, some cross modules,
/// and a cycle, a rename and a new module appear along the way. Nothing here
/// is a claim about real projects: it exists to demonstrate the tool.
pub fn generate_demo_fixture(directory: &Path) -> Result<Oid, git2::Error> {
    const COMMITS: usize = 180;
    let path = |module: &str, file: &str| format!("src/{module}/{file}.ts");
    let mut repo = FixtureRepo::init(directory)?;
    let mut sequence = Sequence(42);

    let mut root = repo.change()?.write("README.md", "# Demo product\n");
    for (index, (module, files)) in DEMO_MODULES[..5].iter().enumerate() {
        for (offset, file) in files.iter().enumerate() {
            let mut content = exports(file, 4);
            if offset > 0 {
                content.insert_str(0, &format!("import {{ {0}0 }} from './{0}';\n", files[0]));
            }
            // Each module's main file builds on the one before it.
            if index > 0 && offset == 0 {
                let (previous, others) = DEMO_MODULES[index - 1];
                content.insert_str(
                    0,
                    &format!("import {{ {0}0 }} from '../{previous}/{0}';\n", others[0]),
                );
            }
            root = root.write(path(module, file), content);
        }
    }
    root.commit("Create the product")?;

    let mut live: Vec<(String, Vec<String>)> = DEMO_MODULES[..5]
        .iter()
        .map(|(module, files)| (module.to_string(), files.map(str::to_owned).to_vec()))
        .collect();

    for number in 1..COMMITS {
        match number {
            45 => {
                let mut change = repo.change()?;
                let (module, files) = DEMO_MODULES[5];
                for file in files {
                    let import = "import { routes0 } from '../api/routes';\n";
                    change =
                        change.write(path(module, file), format!("{import}{}", exports(file, 3)));
                }
                change.commit("Add notifications")?;
                live.push((module.to_owned(), files.map(str::to_owned).to_vec()));
                continue;
            }
            80 => {
                repo.change()?
                    .append(
                        &path("auth", "token"),
                        "import { session0 } from './session';\n",
                    )
                    .append(
                        &path("auth", "session"),
                        "import { token0 } from './token';\n",
                    )
                    .commit("Let tokens read session state")?;
                continue;
            }
            120 => {
                repo.change()?
                    .replace(
                        &path("auth", "token"),
                        "import { session0 } from './session';\n",
                        "",
                    )
                    .commit("Stop tokens reading session state")?;
                continue;
            }
            100 => {
                repo.change()?
                    .rename(&path("users", "profile"), &path("users", "details"))
                    .commit("Rename profile to details")?;
                live[2].1[1] = "details".to_owned();
                continue;
            }
            _ => {}
        }

        let module = sequence.below(live.len());
        let touched = 2 + sequence.below(2);
        let start = sequence.below(4);
        let mut change = repo.change()?;
        let mut names = Vec::new();
        for offset in 0..touched {
            let file = &live[module].1[(start + offset) % 4];
            let line = format!("export const change{number}x{offset} = {number};\n");
            change = change.append(&path(&live[module].0, file), &line);
            names.push(file.clone());
        }
        // Auth and billing move together often, across modules.
        if module == 0 && sequence.below(2) == 0 {
            let line = format!("export const follow{number} = {number};\n");
            change = change.append(&path("billing", "invoice"), &line);
        }
        change.commit(&format!(
            "Update {} in {}",
            names.join(" and "),
            live[module].0
        ))?;
    }
    Ok(repo.head.expect("the fixture has commits"))
}

/// A repository of `files` source files in `directories` of 40 and `commits`
/// further small commits, to measure scale. Its shape is arbitrary.
pub fn generate_scale_fixture(
    directory: &Path,
    files: usize,
    commits: usize,
) -> Result<Oid, git2::Error> {
    const PER_DIRECTORY: usize = 40;
    const DIRECTORIES_PER_PACKAGE: usize = 25;
    let path = |index: usize| {
        let folder = index / PER_DIRECTORY;
        format!(
            "packages/pkg{:03}/src/mod{:03}/file{index}.ts",
            folder / DIRECTORIES_PER_PACKAGE,
            folder
        )
    };
    let relative = |index: usize| {
        format!(
            "../../../pkg{:03}/src/mod{:03}/file{index}",
            index / PER_DIRECTORY / DIRECTORIES_PER_PACKAGE,
            index / PER_DIRECTORY
        )
    };

    let mut repo = FixtureRepo::init(directory)?;
    let mut sequence = Sequence(7);
    let mut root = repo.change()?;
    for index in 0..files {
        let mut content = String::new();
        for _ in 0..3.min(index) {
            let target = sequence.below(index);
            content.push_str(&format!(
                "import {{ v{target} }} from '{}';\n",
                relative(target)
            ));
        }
        content.push_str(&format!("export const v{index} = {index};\n"));
        root = root.write(path(index), content);
    }
    root.commit("Create the packages")?;

    for number in 0..commits {
        let mut change = repo.change()?;
        let first = sequence.below(files);
        for offset in 0..3 {
            let line = format!("export const change{number}x{offset} = {number};\n");
            change = change.append(&path((first + offset) % files), &line);
        }
        change.commit(&format!("Change {number}"))?;
    }
    Ok(repo.head.expect("the fixture has commits"))
}
