# Commits and publication

Keep mechanical refactors separate from behavioral or performance changes. A module
move, rename, or formatting change must not silently alter public behavior.

Use descriptive, potentially multiline commit messages for substantial changes. Do
not create a commit or push it unless the user requests that action. A request to
commit does not imply permission to push.

Preserve unrelated working-tree changes. Review the complete diff before committing,
and keep generated build output and temporary benchmark data out of the commit.
