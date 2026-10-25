import {Config} from '@remotion/cli/config';
Config.setBrowserExecutable(process.env.CHROME_BIN || null);
Config.setConcurrency(2);
Config.setVideoImageFormat('jpeg');
Config.setJpegQuality(88);
