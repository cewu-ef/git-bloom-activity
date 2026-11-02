# git-bloom-activity

git-bloom-activity grows a pot of pink pixel-art flowers from one repository's
commit history. Its commits grow the plant and add flowers, its languages color
the back petals and a band on the pot, its weekly streaks add leaves and shine,
and the day that just ended decides how it moves. A nightly workflow puts the
pot in the repository's README as a looping GIF.

<img src="docs/stages.png" width="640" alt="Top row: seed, sprout, bud, pale young flower, and a second flower arriving. Middle row: three and five flowers, then a pot swaying, bursting with pollen, and unfolding. Bottom row: resting, wilting, and three pots for other language mixes. Every pot has pink front petals, language-colored petals behind them, and language stripes on the pot." />

## What the flower shows

| Repository                  | Flower                                                                                                                                                                                                                               |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Total commits               | Seed at 0, sprout from 1, bud from 10, open flower from 40; keeps growing with diminishing gains up to 10,000                                                                                                                        |
| More commits                | A second flower buds at 150, a third at 600, a fourth at 2,000, and a fifth at 6,000; each opens at half again its threshold                                                                                                         |
| Languages                   | Petals in front are always pink. Behind them, a ring of petals takes the top four languages' GitHub colors, each language getting petals in proportion to its share of the code. A striped band around the pot shows the same shares |
| Young repositories          | Petals and stripes show pale until 250 commits                                                                                                                                                                                       |
| Current weekly streak       | One extra leaf per two active weeks, up to four                                                                                                                                                                                      |
| Longest weekly streak       | Petals grow more vivid up to a year; past half a year they glint                                                                                                                                                                     |
| Yesterday's activity (mood) | See below                                                                                                                                                                                                                            |

A week counts toward a streak when it has at least one commit, Monday through
Sunday. Merge commits and commits by GitHub Apps, such as this workflow's own
commits and Dependabot, are left out. Dates are author dates in UTC.

| Mood      | When                                                                                                                              |
| --------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Wilting   | 60 or more days without commits                                                                                                   |
| Resting   | 14 or more days without commits                                                                                                   |
| Unfolding | The first day with commits in its week                                                                                            |
| Bursting  | Within a week of passing 40, 100, 500, 1k, 2.5k, 5k, or 10k commits, or a Sunday ending one of the busiest weeks of the last year |
| Swaying   | Any other day                                                                                                                     |

## Add it to a repository

1. Copy [examples/bloom-workflow.yml](examples/bloom-workflow.yml) into the
   repository as `.github/workflows/bloom.yml`.
2. Embed the flower in the repository's `README.md`:

   ```html
   <img src="bloom/bloom.gif" width="256" alt="This repository's flower" />
   ```

3. Run the workflow once from the Actions tab, so the first flower appears
   without waiting for midnight.

The workflow runs just after midnight UTC and whenever its file changes. It
needs `fetch-depth: 0` on checkout, since the flower reads the whole history.
Its built-in token is enough to read the languages, private repositories
included, and to push the updated GIF. It commits only when the GIF changed.

| Input        | Default            | Meaning                                                       |
| ------------ | ------------------ | ------------------------------------------------------------- |
| `output-dir` | `bloom`            | Folder that receives `bloom.gif`, `bloom.png`, `preview.html` |
| `repository` | The current one    | Repository whose languages to read, as `OWNER/NAME`           |
| `token`      | The built-in token | Token for reading the repository's languages                  |

Things to know:

- **Protected branches.** The commit step pushes to the branch it checked out.
  If that branch requires pull requests, the push fails; point the workflow at
  an unprotected branch, or allow the Actions bot to bypass the rule.
- **Build time.** The Action compiles itself on each run, which takes under a
  minute on `ubuntu-latest`. Publishing prebuilt binaries on release tags would
  remove that step.
- **Scheduled runs pause.** GitHub pauses scheduled workflows after 60 days
  without repository activity. Re-enable it from the Actions tab.

## Demo it to a team

Run it on a laptop against a local clone of the team's repository. Nothing is
pushed, and each render takes about a second. You need Rust and a GitHub CLI
signed in with access to the repository.

1. Build once before the meeting:

   ```sh
   cargo build --release
   ```

2. Draw the team repository's flower as of yesterday and open the preview, which
   shows it on GitHub's light and dark backgrounds:

   ```sh
   ./target/release/git-bloom-activity render \
     --repo-dir ../path/to/team-repo \
     --repository ORG/TEAM-REPO \
     --output-dir /tmp/demo-bloom
   open /tmp/demo-bloom/preview.html
   ```

3. Replay earlier days with `--observed-on YYYY-MM-DD`. Commits count only up to
   that day, so an early date shows a sprout or bud, and a date after a quiet
   stretch shows resting or wilting. Languages always come from the repository
   as it is today.

4. Show every stage and mood at once with [docs/stages.png](docs/stages.png).

To prove the Action runs under the organization's settings without writing to
the repository, push a test branch with a read-only workflow that uploads the
result as a run artifact. Replace `TEST-BRANCH` with the branch name:

```yaml
name: Bloom test

on:
  push:
    branches: [TEST-BRANCH]

permissions:
  contents: read

jobs:
  bloom:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
        with:
          fetch-depth: 0
      - uses: cewu-ef/git-bloom-activity@main
        with:
          output-dir: bloom
      - uses: actions/upload-artifact@v7
        with:
          name: bloom
          path: bloom/
```

Download the `bloom` artifact from the run and open `preview.html`. Afterwards,
delete the branch with `git push origin --delete TEST-BRANCH`, and the run with
`gh run delete RUN-ID` if it should not stay in the Actions tab.

## Roll it out to a team project

1. Open a pull request against the default branch that adds
   [examples/bloom-workflow.yml](examples/bloom-workflow.yml) as
   `.github/workflows/bloom.yml`, and this line to the `README.md`:

   ```html
   <img src="bloom/bloom.gif" width="256" alt="This repository's flower" />
   ```

   The image stays broken until the first run commits the GIF.

2. Before merging, have the reviewers confirm three settings:
   - **Branch protection.** The workflow's bot pushes one commit to the default
     branch each night the flower changes. If the branch requires pull requests,
     allow GitHub Actions to bypass that rule.
   - **Workflow permissions.** The workflow asks for `contents: write`. An
     organization that caps tokens at read-only must allow read and write.
   - **Allowed actions.** An organization that limits outside actions must allow
     `cewu-ef/git-bloom-activity`.

3. After merging, run the workflow once from the Actions tab so the first flower
   appears without waiting for midnight UTC. It then redraws nightly.

The bot's commits never count toward the flower, since commits by GitHub Apps
are left out.

## Developing

```sh
cargo test
cargo run -- sheet --output docs/stages.png
cargo run -- render --repo-dir ../some-repo --repository OWNER/NAME --output-dir bloom
```

`render` needs an authenticated [GitHub CLI](https://cli.github.com) for the
language query. Open `bloom/preview.html` to see the result on GitHub's light
and dark page colors.

## Credits

The pipeline, the weekly streak rules, and the GIF and PNG encoders are adapted
from [git-koi](https://github.com/yalongwastaken/git-koi) by Anthony Yalong,
under the MIT License.

## License

MIT. See [LICENSE](LICENSE).
