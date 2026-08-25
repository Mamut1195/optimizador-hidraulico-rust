#requires -version 5.1
# Build both mdBook variants (EN + ES).
# Usage: .\docs-site\build.ps1 [-Clean]
param(
    [switch]$Clean
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

if ($Clean) {
    if (Test-Path "$root\en\book") { Remove-Item "$root\en\book" -Recurse -Force }
    if (Test-Path "$root\es\book") { Remove-Item "$root\es\book" -Recurse -Force }
}

Write-Host "==> Building English book"
mdbook build "$root\en"

Write-Host "==> Building Spanish book"
mdbook build "$root\es"

Write-Host
Write-Host "Done."
Write-Host "  Landing: $root\index.html"
Write-Host "  EN:      $root\en\book\index.html"
Write-Host "  ES:      $root\es\book\index.html"
