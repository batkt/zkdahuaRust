using System;
using System.Runtime.InteropServices;

namespace CameraDemo
{
    public class AlprSDK1
    {

        [StructLayout(LayoutKind.Sequential)]
        public struct XRect
        {
            public int left;
            public int right;
            public int top;
            public int bottom;
        };
        [StructLayout(LayoutKind.Sequential)]
        public struct PLATERESULT
        {
            //[MarshalAs(UnmanagedType.ByValTStr, SizeConst = 24)]
            //public string szLicense;
            [MarshalAs(UnmanagedType.ByValArray, SizeConst = 24)]
            public byte[] szLicense;
            public int nLetterCount;  //字符数量
            public float fConfidence; //车牌的可信度,取值范围[1,1000]. 值越小，可信度越高 -- Credibility of license plate, the value range [1,1000]. smaller the value, the higher the confidence
            public XRect rect;        //车牌位置信息 -- palte location infomation
            public byte plateColor;   //车牌颜色，取值请参考ePlateColor -- palte color, pls refer ePlateColor
            public byte bDoublePlates;//是否双层车牌 1:双层车牌，0：双层车牌(使用byte) -- double layer plate setting

            public byte nDirection;  //前3bit运动方向，取值为MotionDir之一; 1bit假车牌；1bit虚警;其他bit保留
            //public byte nReserve;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 33)]
            public string reserve;
        };

        [StructLayout(LayoutKind.Sequential)]
        public struct LICENSE_PLATE
        {
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 20)]
            public string szTime;
            public int nProcessTime;        //当前图片的识别时间 -- image recognition time
            public int nPlateNum;           //当前识别的车牌数量 -- number of license plate recognition
            [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
            public PLATERESULT[] pPlate;    //一次最多可以识别4张车牌 -- Up to four license plates can be identified at a time

        };

        [StructLayout(LayoutKind.Sequential)]
        public struct XDate
        {
            public ushort nYear;  //取值是完整的年，例如2016 -- In 2016, for example
            public byte nMonth;   //月取值1,2，...， 12 --  monthly value is 1,2..., 12
            public byte nDay;     //日取值：1,2,...,31  --  day value is 1,2..., 12
        };

        [StructLayout(LayoutKind.Sequential)]
        public struct PLATE_INFO_EXT
        {
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
            public string szPlate;
            public XDate fromData;
            public XDate toDate;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
            public string szName;

            public long CarNo;
            public byte nPlateColor;
            [MarshalAs(UnmanagedType.ByValArray, SizeConst = 15)]
            public byte[] Reserve;

        }

        public struct DEVINFO
        {
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
            public string szIP;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
            public string szDevName;

            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
            public string szDevUid;
            public ushort uUseP2PConn;

            public ushort u16Port;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
            public string szUser;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
            public string szPwd;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
            public string szPicturesSavePath;  //图片本地保存路径 -- image save path

            public UInt16 u16AlprPort;
            public ushort lprDevType;//增加设备类型 -- device type
            IntPtr hPullHandle;

            public void Init(string devName, string ip, ushort port, string userName, string password, ushort Devtype)
            {
                szIP = ip;
                szDevName = devName;
                u16Port = port;
                szUser = userName;
                szPwd = password;
                szPicturesSavePath = "";
                lprDevType = Devtype;//增加设备类型 -- device type
            }

        };

        public enum EAPIClientType
        {
            E_CLIENT_NORMAL = 0,	//normal client 
            E_CLIENT_DEV_OCX,		//use for webserver of device
            E_CLIENT_DEMO,         //use for testing demo
        };

        public enum ELPRDevType
        {
            LPR_DEV_UNKNOWN = 0,		/**< 未知设备	unknown device*/
            LPR_DEV_JL = 1,				/**< 巨龙车牌设备 JL device   */
            LPR_DEV_GZ = 2,				/**< 感知车牌设备 GZ device   */
            LPR_DEV_GZ_CAR_SPACE = 3,	/**< 感知车位设备 GZ space device	  */
        };


        [StructLayout(LayoutKind.Sequential)]
        public struct JPG_BYTES
        {
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 20)]
            public string szTime;   //捕获的时间,格式为：20161012163417050,即2016年10月12日16点34分17秒050毫秒 -- Identify time elapsed
            public int nBytesLen;    //jpg数据长度 -- jpg data length
            public IntPtr pJpgBytes;    //jpg数据，可直接保存为jpg文件 -- jpg data
        }

        [StructLayout(LayoutKind.Sequential)]
        public struct RECOG_ALL_INFO
        {
            public LICENSE_PLATE PlateInfo; //识别的车牌信息 -- plate infomation
            public JPG_BYTES JpgBytes;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
            public string nReserve; //预留字段
        }




        [StructLayout(LayoutKind.Sequential)]
        public struct CARSPACEINFO
        {
            //[MarshalAs(UnmanagedType.ByValTStr, SizeConst = 24)]
            //public string szLicense;
            [MarshalAs(UnmanagedType.ByValArray, SizeConst = 24)]
            public byte[] szLicense;
            public int nLetterCount;  //字符数量
            public float fConfidence; //车牌的可信度,取值范围[1,1000]. 值越小，可信度越高 -- Credibility of license plate, the value range [1,1000]. smaller the value, the higher the confidence
            public XRect rect;        //车牌位置信息 -- palte location infomation
            public byte plateColor;   //车牌颜色，取值请参考ePlateColor -- palte color, pls refer ePlateColor
            public byte bDoublePlates;//是否双层车牌 1:双层车牌，0：双层车牌(使用byte) -- double layer plate setting

            public byte nDirection;  //前3bit运动方向，取值为MotionDir之一; 1bit假车牌；1bit虚警;其他bit保留
            public byte nReserve;
            public int nCarSpaceID;		    /**< 车位ID												*/
            public int nCarSpaceState;	    /**< 车位状态											*/
            public int nIntResv1;
            public int nIntResv2;
            public int nIntResv3;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
            public string reserve;
        };




        [StructLayout(LayoutKind.Sequential)]
        public struct CAR_SPACES
        {
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 20)]
            public string szTime;
            public int nProcessTime;           //当前图片的识别时间 -- image recognition time
            public int nCarSpaceNum;           //当前识别的车位数量 -- number of license plate recognition
            [MarshalAs(UnmanagedType.ByValArray, SizeConst = 6)]
            public CARSPACEINFO[] pPlate;    //一次最多可以识别4车位 -- Up to four license plates can be identified at a time

        };


        [StructLayout(LayoutKind.Sequential)]
        public struct CAR_SPACE_INFO
        {
            public CAR_SPACES CarSpaces; //识别的车位信息
            public JPG_BYTES JpgBytes;  //识别的
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
            public string nReserve; //预留字段
        }


        private const string dllname = "AlprSDK1.dll";

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_Startup(IntPtr hNotifyWnd, UInt32 nCommandID);

        public delegate void ServerFindCallback(int nDeviceType, string pDeviceName,
        string pIP, IntPtr macAddr, ushort wPortWeb, ushort wPortListen, string pSubMask,
        string pGateway, string pMultiAddr, string pDnsAddr, ushort wMultiPort,
        int nChannelNum, int nFindCount, int dwDeviceID);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_SearchAllCameras(UInt32 nTimeout, ServerFindCallback callback);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_InitHandle(int nHandleID, IntPtr hVideoWnd);
        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_UnInitHandle(int nHandleID);

        public delegate void RecogResultCallback(ref LICENSE_PLATE plateInfo);
        public delegate void CarSpaceStateCallbackFun(ref CAR_SPACE_INFO pCarSpaceInfo, IntPtr pUserData);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_ConnectDev(int nHandleID, ref DEVINFO pDev, EAPIClientType type);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_DisConnectDev(int nHandleID);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_StartVideo(int nHandleID);
        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_StopVideo(int nHandleID);

        public delegate void RecogAllInfoCallback(ref RECOG_ALL_INFO pRecogAllInfo, IntPtr pUserData);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_CreateRecogAllInfoTask(int nHandleID, RecogAllInfoCallback callback, IntPtr pUserData);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_ClearRecogAllInfoTask(int nHandleID);


        public delegate void CarSpaceStateCallback(ref CAR_SPACE_INFO pCarSpaceInfo, IntPtr pUserData);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_CreateCarSpaceTask(int nHandleID, CarSpaceStateCallback callback, IntPtr pUserData);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_ClearCarSpaceTask(int nHandleID);



        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_SendHeartBeat(int nHandleID);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_SetConnectTimeout(int nHandleID, int nTimeout);
        public delegate void DevStatusCallback(IntPtr pBuf, ref int len, IntPtr pUserData);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_CaptureJpg(int nHandleID);
        public delegate void DeviceCaptureCallback(IntPtr pBuf, ref int len);
        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_CreateCaptureJpgCallback(int nHandleID, DeviceCaptureCallback callback);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_OpenGate(int nHandleID);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_SetCarTeamSignal(int nHandleID, int signal);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_CommTransparentTransfer(int nHandleID, byte[] pdata, int len);

        [DllImport(dllname, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Unicode)]
        public static extern int AlprSDK_Trans2Screen(int nHandleID, int nScreenType, int nFirstLineColor, byte[] pFirstLineData, int nSecondLineColor, byte[] pSecondLineData, int nThirdLineColor, byte[] pThirdLineData, int nFourthLineColor, byte[] pFourthLineData);
    }
}
