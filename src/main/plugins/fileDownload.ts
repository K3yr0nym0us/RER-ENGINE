import fs from 'fs'
import https from 'https'
import http from 'http'
import path from 'path'

export interface DownloadProgress {
  bytesReceived: number
  bytesTotal: number
  percent: number
}

export async function downloadFile(
  url: string,
  destPath: string,
  onProgress?: (progress: DownloadProgress) => void,
  expectedMinBytes?: number,
): Promise<void> {
  fs.mkdirSync(path.dirname(destPath), { recursive: true })
  const tempPath = `${destPath}.part`

  await downloadToTemp(url, tempPath, onProgress)

  const stat = fs.statSync(tempPath)
  if (expectedMinBytes != null && stat.size < expectedMinBytes * 0.9) {
    fs.unlinkSync(tempPath)
    throw new Error(`Downloaded file is too small (${stat.size} bytes)`)
  }

  if (fs.existsSync(destPath)) fs.unlinkSync(destPath)
  fs.renameSync(tempPath, destPath)
}

function downloadToTemp(
  url: string,
  destPath: string,
  onProgress?: (progress: DownloadProgress) => void,
  redirectCount = 0,
): Promise<void> {
  if (redirectCount > 10) {
    return Promise.reject(new Error('Too many redirects'))
  }

  return new Promise((resolve, reject) => {
    const protocol = url.startsWith('https:') ? https : http

    const request = protocol.get(url, (response) => {
      const status = response.statusCode ?? 0

      if (status >= 300 && status < 400 && response.headers.location) {
        const next = new URL(response.headers.location, url).toString()
        response.resume()
        downloadToTemp(next, destPath, onProgress, redirectCount + 1).then(resolve).catch(reject)
        return
      }

      if (status !== 200) {
        response.resume()
        reject(new Error(`Download failed with status ${status}`))
        return
      }

      const total = Number(response.headers['content-length'] ?? 0)
      let received = 0

      const file = fs.createWriteStream(destPath)
      response.on('data', (chunk: Buffer) => {
        received += chunk.length
        if (onProgress) {
          const percent = total > 0 ? Math.min(100, Math.round((received / total) * 100)) : 0
          onProgress({ bytesReceived: received, bytesTotal: total, percent })
        }
      })

      response.pipe(file)
      file.on('finish', () => {
        file.close(() => resolve())
      })
      file.on('error', (err) => {
        fs.unlink(destPath, () => reject(err))
      })
    })

    request.on('error', reject)
  })
}
