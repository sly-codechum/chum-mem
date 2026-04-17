# WHY: PowerShell module for deployment automation. We use functions
# instead of raw scripts so each step is independently testable.

Import-Module Az.Storage
Import-Module Az.KeyVault

function Get-DeployConfig {
    <#
    .SYNOPSIS
        Load deployment configuration from environment and defaults.
    .DESCRIPTION
        Merges environment overrides with built-in defaults.
        NOTE: DEPLOY_ENV must be set or this will throw.
    #>
    param(
        [Parameter(Mandatory = $false)]
        [string]$Environment = $env:DEPLOY_ENV
    )

    if (-not $Environment) {
        throw "DEPLOY_ENV is not set"
    }
    return @{
        Env      = $Environment
        Region   = "us-east-1"
        Timeout  = 300
    }
}

function Invoke-Deployment {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable]$Config
    )

    Write-Host "Deploying to $($Config.Env) in $($Config.Region)"
    $vault = Get-AzKeyVault -VaultName "deploy-$($Config.Env)"
    $secret = Get-AzKeyVaultSecret -VaultName $vault.VaultName -Name "api-key"
    Write-Host "Retrieved secret, proceeding with deployment"
    return @{ Status = "success"; Timestamp = Get-Date -Format "o" }
}

$config = Get-DeployConfig -Environment "staging"
$result = Invoke-Deployment -Config $config
Write-Host "Result: $($result.Status)"
