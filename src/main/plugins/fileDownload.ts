import fs from 'fs'
import https from 'https'
import http from 'http'
import type { ClientRequest } from 'http'
import path from 'path'

export interface DownloadProgress {
  bytesReceived: number
  bytesTotal: number
  percent: number
}

export class DownloadCancelledError extends Error {
  constructor() {
    super('Download cancelled')
    this.name = 'DownloadCancelledError'
  }
}

let activeRequest: ClientRequest | null = null
let activeWriteStream: fs.WriteStream | null = null
let cancelRequested = false

export function cancelActiveDownload(): void {
  cancelRequested = true
  if (activeRequest) {
    activeRequest.destroy()
    activeRequest = null
  }
  if (activeWriteStream) {
    activeWriteStream.destroy()
    activeWriteStream = null
  }
}

export function resetDownloadCancelState(): void {
  cancelRequested = false
  activeRequest = null
  activeWriteStream = null
}

function throwIfCancelled(): void {
  if (cancelRequested) {
    throw new DownloadCancelledError()
  }
}

export async function downloadFile(
  url: string,
  destPath: string,
  onProgress?: (progress: DownloadProgress) => void,
  expectedMinBytes?: number,
  expectedTotalBytes?: number,
): Promise<void> {
  fs.mkdirSync(path.dirname(destPath), { recursive: true })
  const tempPath = `${destPath}.part`

  try {
    await downloadToTemp(url, tempPath, onProgress, 0, expectedTotalBytes)
    throwIfCancelled()

    const stat = fs.statSync(tempPath)
    if (expectedMinBytes != null && stat.size < expectedMinBytes * 0.9) {
      fs.unlinkSync(tempPath)
      throw new Error(`Downloaded file is too small (${stat.size} bytes)`)
    }

    if (fs.existsSync(destPath)) fs.unlinkSync(destPath)
    fs.renameSync(tempPath, destPath)
  } catch (err) {
    try {
      if (fs.existsSync(tempPath)) fs.unlinkSync(tempPath)
    } catch {
      // ignore
    }
    throw err
  }
}

function downloadToTemp(
  url: string,
  destPath: string,
  onProgress?: (progress: DownloadProgress) => void,
  redirectCount = 0,
  expectedTotalBytes?: number,
): Promise<void> {
  if (redirectCount > 10) {
    return Promise.reject(new Error('Too many redirects'))
  }

  throwIfCancelled()

  return new Promise((resolve, reject) => {
    const protocol = url.startsWith('https:') ? https : http

    const request = protocol.get(url, (response) => {
      activeRequest = request
      const status = response.statusCode ?? 0

      if (status >= 300 && status < 400 && response.headers.location) {
        const next = new URL(response.headers.location, url).toString()
        response.resume()
        downloadToTemp(next, destPath, onProgress, redirectCount + 1, expectedTotalBytes)
          .then(resolve)
          .catch(reject)
        return
      }

      if (status !== 200) {
        response.resume()
        reject(new Error(`Download failed with status ${status}`))
        return
      }

      const headerTotal = Number(response.headers['content-length'] ?? 0)
      const total = headerTotal > 0 ? headerTotal : (expectedTotalBytes ?? 0)
      let received = 0

      const file = fs.createWriteStream(destPath)
      activeWriteStream = file

      response.on('data', (chunk: Buffer) => {
        if (cancelRequested) {
          response.destroy()
          file.destroy()
          reject(new DownloadCancelledError())
          return
        }
        received += chunk.length
        if (onProgress) {
          const percent = total > 0 ? Math.min(100, Math.round((received / total) * 100)) : 0
          onProgress({ bytesReceived: received, bytesTotal: total, percent })
        }
      })

      response.pipe(file)
      file.on('finish', () => {
        activeRequest = null
        activeWriteStream = null
        file.close(() => {
          if (cancelRequested) {
            reject(new DownloadCancelledError())
            return
          }
          resolve()
        })
      })
      file.on('error', (err) => {
        activeRequest = null
        activeWriteStream = null
        fs.unlink(destPath, () => reject(err))
      })
    })

    request.on('error', (err) => {
      activeRequest = null
      activeWriteStream = null
      if (cancelRequested) {
        reject(new DownloadCancelledError())
        return
      }
      reject(err)
    })
  })
}
