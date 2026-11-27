// Settings: a tab per area along the top; each tab picks its sections on the left.
import { Box, Tabs, Tab, IconButton, Tooltip } from '@mui/material';
import ArrowBackIcon from '@mui/icons-material/ArrowBack';
import { useNavigation } from '../hooks/useNavigation';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import ProxySettings from '../components/ProxySettings';
import GeneralSettings from '../components/GeneralSettings';
import AdvancedSettings from '../components/AdvancedSettings';
import DownloadSettings from '../components/DownloadSettings';
import Toast from '../components/Toast';
import { useAtom } from 'jotai';
import { settingsNoticeAtom } from '../hooks/useUpdateSettings';
import ModulesSettings from '../components/modules/ModulesSettings';
import PageLayout from '../components/PageLayout';

interface TabPanelProps {
  children?: React.ReactNode;
  index: number;
  value: number;
}

function TabPanel(props: TabPanelProps) {
  const { children, value, index, ...other } = props;

  return (
    <div
      role="tabpanel"
      hidden={value !== index}
      id={`settings-tabpanel-${index}`}
      aria-labelledby={`settings-tab-${index}`}
      {...other}
    >
      {value === index && <Box sx={{ p: 3, maxWidth: 1040, mx: 'auto' }}>{children}</Box>}
    </div>
  );
}

export default function SettingsPage() {
  const { t } = useTranslation();
  const [currentTab, setCurrentTab] = useState(0);
  const { navigateTo } = useNavigation();
  const [notice, setNotice] = useAtom(settingsNoticeAtom);

  const handleTabChange = (_event: React.SyntheticEvent, newValue: number) => {
    setCurrentTab(newValue);
  };

  return (
    <PageLayout
      header={
        <>
          <Tooltip title={t('common.back')}>
            <IconButton onClick={() => navigateTo('main')}>
              <ArrowBackIcon fontSize="small" />
            </IconButton>
          </Tooltip>
          <Tabs value={currentTab} onChange={handleTabChange} sx={{ flex: 1, alignSelf: 'flex-end' }}>
            <Tab label={t('settings.general')} />
            <Tab label={t('settings.downloads')} />
            <Tab label={t('settings.proxy')} />
            <Tab label={t('modules.title')} />
            <Tab label={t('settings.advanced')} />
          </Tabs>
        </>
      }
    >
      <TabPanel value={currentTab} index={0}>
        <GeneralSettings />
      </TabPanel>

      <TabPanel value={currentTab} index={1}>
        <DownloadSettings />
      </TabPanel>

      <TabPanel value={currentTab} index={2}>
        <ProxySettings />
      </TabPanel>

      <TabPanel value={currentTab} index={3}>
        <ModulesSettings />
      </TabPanel>

      <TabPanel value={currentTab} index={4}>
        <AdvancedSettings />
      </TabPanel>
      <Toast message={notice} onClose={() => setNotice(null)} />
    </PageLayout>
  );
}
