#requires -version 5.1
# Live-preview a book with auto-reload on file change.
# Usage: .\docs-site\serve.ps1 [-Lang en|es]   (default: en)
param(
    [ValidateSet('en', 'es')]
    [string]$Lang = 'en'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "==> Serving $Lang book on http://localhost:3000"
mdbook serve "$root\$Lang" --open
