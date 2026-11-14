import numpy as np, pydicom, random, sys, os
from pydicom.dataset import Dataset, FileMetaDataset
from pydicom.uid import ExplicitVRLittleEndian, generate_uid, CTImageStorage
out=sys.argv[1]; os.makedirs(out,exist_ok=True)
st,se=generate_uid(),generate_uid(); N=80; R=256
yy,xx=np.mgrid[0:R,0:R]; cx=cy=R/2
names=list(range(N)); random.seed(1); random.shuffle(names)
for i in range(N):
    z=-200+i*2.5
    img=np.full((R,R),-1000.0)
    body=((xx-cx)/110)**2+((yy-cy)/85)**2<1; img[body]=40
    for sx in (-45,45):
        lung=((xx-cx-sx)/32)**2+((yy-cy+5)/45)**2<1; img[lung]=-850
    spine=((xx-cx)/12)**2+((yy-cy-55)/12)**2<1; img[spine]=700
    r=10+8*np.sin(i/N*np.pi); les=((xx-cx-45)**2+(yy-cy+10)**2)<r*r; img[les]=60
    img+=np.random.default_rng(i).normal(0,8,img.shape)
    raw=np.clip(img+1024,0,4095).astype(np.uint16)
    m=FileMetaDataset(); m.MediaStorageSOPClassUID=CTImageStorage; m.MediaStorageSOPInstanceUID=generate_uid(); m.TransferSyntaxUID=ExplicitVRLittleEndian
    d=Dataset(); d.file_meta=m
    d.SOPClassUID=CTImageStorage; d.SOPInstanceUID=m.MediaStorageSOPInstanceUID
    d.PatientName="PHANTOM^CHEST"; d.PatientID="RD0001"; d.PatientBirthDate="19700101"
    d.StudyInstanceUID=st; d.SeriesInstanceUID=se; d.StudyDate="20261015"; d.StudyDescription="CT TORAX"; d.SeriesDescription="Axial 2.5mm"; d.AccessionNumber="A123"
    d.Modality="CT"; d.SeriesNumber=2; d.InstanceNumber=i+1
    d.ImagePositionPatient=[-160,-160,z]; d.ImageOrientationPatient=[1,0,0,0,1,0]; d.SliceLocation=z; d.SliceThickness=2.5
    d.PixelSpacing=[1.25,1.25]; d.Rows=R; d.Columns=R; d.SamplesPerPixel=1; d.PhotometricInterpretation="MONOCHROME2"
    d.BitsAllocated=16; d.BitsStored=12; d.HighBit=11; d.PixelRepresentation=0
    d.RescaleIntercept=-1024; d.RescaleSlope=1; d.WindowCenter=40; d.WindowWidth=400
    d.PixelData=raw.tobytes()
    d.save_as(os.path.join(out,f"IM{names[i]:04d}"),enforce_file_format=True)
print("ok",N)
