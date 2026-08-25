/**
 * Runs SonarScanner when SONAR_TOKEN is available.
 * Local `yarn quality` does not require Sonar; CI enforces the Quality Gate.
 */
import { spawnSync } from 'node:child_process'
import process from 'node:process'

const token = process.env.SONAR_TOKEN
if (!token) {
  console.error(
    '[sonar] SONAR_TOKEN is not set. Configure the token (and SONAR_ORGANIZATION) to run analysis.',
  )
  process.exit(1)
}

const args = [
  '-Dsonar.token=' + token,
  '-Dsonar.qualitygate.wait=true',
]

if (process.env.SONAR_ORGANIZATION) {
  args.push('-Dsonar.organization=' + process.env.SONAR_ORGANIZATION)
}
if (process.env.SONAR_HOST_URL) {
  args.push('-Dsonar.host.url=' + process.env.SONAR_HOST_URL)
}
if (process.env.SONAR_PROJECT_KEY) {
  args.push('-Dsonar.projectKey=' + process.env.SONAR_PROJECT_KEY)
}

const result = spawnSync(
  'yarn',
  ['dlx', 'sonarqube-scanner', ...args],
  { stdio: 'inherit', shell: true },
)

process.exit(result.status === null ? 1 : result.status)
