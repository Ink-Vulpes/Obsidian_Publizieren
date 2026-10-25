# Baut das Docker-Image lokal und pusht es nach ghcr.io/ink-vulpes/obsidian-publisher
# Verwendung: .\push-image.ps1          (Tag: latest)
#             .\push-image.ps1 1.0.0    (Tag: 1.0.0 + latest)

param(
    [string]$Tag = "latest"
)

$Image = "ghcr.io/ink-vulpes/obsidian-publisher"

Write-Host "Baue $Image`:$Tag ..." -ForegroundColor Cyan
docker build -t "${Image}:${Tag}" .
if (-not $?) { exit 1 }

if ($Tag -ne "latest") {
    docker tag "${Image}:${Tag}" "${Image}:latest"
    if (-not $?) { exit 1 }
}

Write-Host "Pushe $Image`:$Tag ..." -ForegroundColor Cyan
docker push "${Image}:${Tag}"
if (-not $?) { exit 1 }

if ($Tag -ne "latest") {
    Write-Host "Pushe $Image`:latest ..." -ForegroundColor Cyan
    docker push "${Image}:latest"
    if (-not $?) { exit 1 }
}

Write-Host "Fertig: $Image`:$Tag" -ForegroundColor Green
