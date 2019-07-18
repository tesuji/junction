#!/bin/sh
set -ex

# setup ssh-agent and provide the GitHub deploy key
eval "$(ssh-agent -s)"
mkdir -p "$HOME/.ssh"
SSH_FILE=$(mktemp -u "$HOME/.ssh/XXXXX")
# use this to encrypt
#    openssl enc -aes-256-cbc -K $password -iv $iv -in id_ed25519 -out id_ed25519.enc -md sha256
# with
#    password=$(openssl rand -hex 32)
#    iv=$(openssl rand -hex 16)
set +x
openssl enc -aes-256-cbc -K "$ENCRYPTED_KEY" -iv "$ENCRYPTED_IV" -in ci/id_ed25519.enc -out "$SSH_FILE" -d -md sha256
set -x
chmod 600 "$SSH_FILE"
ssh-add "$SSH_FILE"
ssh-keyscan github.com > ~/.ssh/known_hosts

cd target/doc
du -sch

# configure git
git init
git config --local user.name "Azure Pipelines"
git config --local user.email "azuredevops@microsoft.com"
git config --local core.autocrlf input

# commit the assets in target/doc to the gh-pages branch and push to GitHub using SSH
git remote add origin git@github.com:lzutao/junction.git

git add .
git commit --quiet -m 'Publishing GitHub Pages'

git push --force origin HEAD:gh-pages
