// The site's fixed facts, in one place.

/** The site's one address. */
export const ORIGIN = 'https://loom.rip';

/** loom's repository. */
export const GITHUB = 'https://github.com/aaen-studios/loom';

/** The user guides, which stay on GitHub. */
export const GUIDES = `${GITHUB}/tree/main/site/docs`;

/** The newest release's files: each stays the newest without the site being built again. */
export const LATEST = `${GITHUB}/releases/latest/download`;

/** The desktop apps' downloads in a release: Windows' installer and the Mac's disk image. */
export const WINDOWS_APP = 'loom-setup.exe';
export const MAC_APP = 'loom-mac.dmg';

/** What a server runs to install loom. The web server sends it on to the newest release's script. */
export const INSTALL = `curl -fsSL ${ORIGIN}/install.sh | sudo sh`;

/** The Docker image, and the command that runs it. */
export const IMAGE = 'ghcr.io/aaen-studios/loom';
export const DOCKER = `docker run -d --name loom -p 127.0.0.1:8790:8790 -v loom-data:/data ${IMAGE}`;

/** The studio, for the footer's credit and the structured data. */
export const STUDIO = { name: 'Aaen Studios', url: 'https://aaenz.no/' };
