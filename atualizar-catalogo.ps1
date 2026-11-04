$ErrorActionPreference='Stop'
$packages=@{}
Get-AppxPackage | ForEach-Object {$packages[$_.PackageFamilyName]=$_}
$catalog=@(foreach($app in Get-StartApps){
 $icon=$null
 if($app.AppID.Contains('!')){
  $parts=$app.AppID.Split('!')
  $package=$packages[$parts[0]]
  if($package -and $package.InstallLocation){
   try{
    [xml]$manifest=Get-Content -LiteralPath (Join-Path $package.InstallLocation 'AppxManifest.xml')
    $application=$manifest.SelectNodes("//*[local-name()='Application']") | Where-Object Id -EQ $parts[1] | Select-Object -First 1
    if($application){
     $visual=$application.SelectSingleNode("./*[local-name()='VisualElements']")
     if($visual){
      $logo=$visual.GetAttribute('Square44x44Logo')
      if($logo){
       $candidate=Join-Path $package.InstallLocation $logo
       if(Test-Path -LiteralPath $candidate){$icon=$candidate}
       else{
        $directory=Split-Path $candidate
        $base=[IO.Path]::GetFileNameWithoutExtension($candidate)
        $found=Get-ChildItem -LiteralPath $directory -Filter ($base+'*.png') -ErrorAction SilentlyContinue | Sort-Object @{Expression={if($_.Name -match 'targetsize-48.*unplated'){0}elseif($_.Name -match 'scale-200'){1}else{2}}} | Select-Object -First 1
        if($found){$icon=$found.FullName}
       }
      }
     }
    }
   }catch{}
  }
 }
 [pscustomobject]@{Name=$app.Name;AppID=$app.AppID;IconPath=$icon}
})
$catalog | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'apps.json') -Encoding UTF8
