[CmdletBinding()]
param([string]$StageDirectory, [string[]]$Minecraft)
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'build-aurora-module.ps1') -Module 'skins' -StageDirectory $StageDirectory -Minecraft $Minecraft
